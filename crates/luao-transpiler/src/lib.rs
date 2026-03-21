pub mod bundler;
pub mod class_emitter;
pub mod emitter;
pub mod enum_emitter;
pub mod expression_emitter;
pub mod formatter;
pub mod mangler;
pub mod minifier;
pub mod runtime;
pub mod statement_emitter;

pub use emitter::Emitter;

#[derive(Debug, Clone, Default)]
pub struct TranspileOptions {
    pub minify: bool,
    pub mangle: bool,
    pub no_self: bool,
    pub mangle_baseclasses: bool,
}

pub fn transpile(source: &str) -> Result<String, Vec<String>> {
    transpile_with_options(source, &TranspileOptions::default())
}

pub fn transpile_with_options(
    source: &str,
    options: &TranspileOptions,
) -> Result<String, Vec<String>> {
    let (ast, parse_errors) = luao_parser::parse(source);
    if !parse_errors.is_empty() {
        return Err(parse_errors.iter().map(|e| e.to_string()).collect());
    }
    let mut resolver = luao_resolver::Resolver::new();
    resolver.mangle_baseclasses = options.mangle_baseclasses;
    let symbol_table = resolver.resolve(&ast);
    let checker = luao_checker::Checker::new(&symbol_table);
    let diagnostics = checker.check(&ast);
    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.severity == luao_checker::DiagnosticSeverity::Error)
        .map(|d| d.to_string())
        .collect();
    if !errors.is_empty() {
        return Err(errors);
    }
    let mangler = if options.mangle {
        Some(mangler::Mangler::new())
    } else {
        None
    };
    let mut emitter = Emitter::new(symbol_table, mangler);
    emitter.no_self = options.no_self;
    emitter.emit(&ast);
    let lua_source = emitter.output();
    if options.minify {
        Ok(formatter::minify_lua(&lua_source, options.no_self))
    } else {
        Ok(formatter::format_lua(&lua_source))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: transpile and assert success, returning the output.
    fn ok(source: &str) -> String {
        transpile(source).unwrap_or_else(|e| panic!("transpile failed: {:?}", e))
    }

    /// Helper: transpile with options and assert success.
    fn ok_with(source: &str, options: &TranspileOptions) -> String {
        transpile_with_options(source, options)
            .unwrap_or_else(|e| panic!("transpile failed: {:?}", e))
    }

    /// Helper: transpile and expect errors, returning the error list.
    fn err(source: &str) -> Vec<String> {
        transpile(source).expect_err("expected transpile to fail")
    }

    // =========================================================================
    // Promise runtime inclusion
    // =========================================================================

    #[test]
    fn async_function_includes_promise_runtime() {
        let out = ok("async function f(): number return 1 end");
        assert!(out.contains("local Promise = {}"), "should include Promise runtime");
        assert!(out.contains("Promise.__index = Promise"), "should set Promise __index");
        assert!(out.contains("function __luao_async(fn)"), "should include __luao_async");
        assert!(out.contains("function __luao_yield(promise)"), "should include __luao_yield");
    }

    #[test]
    fn promise_identifier_includes_runtime() {
        let out = ok("local p = Promise.resolve(42)");
        assert!(out.contains("local Promise = {}"), "Promise identifier should trigger runtime");
    }

    #[test]
    fn no_async_no_promise_no_runtime() {
        let out = ok("local x = 1 + 2");
        assert!(!out.contains("Promise"), "no Promise usage → no runtime");
        assert!(!out.contains("__luao_async"), "no async → no __luao_async");
    }

    #[test]
    fn await_includes_promise_runtime() {
        let out = ok("local x = await someFunc()");
        assert!(out.contains("local Promise = {}"), "await should trigger Promise runtime");
    }

    // =========================================================================
    // Async function emission
    // =========================================================================

    #[test]
    fn async_function_wraps_with_luao_async() {
        let out = ok("async function fetch(): number return 42 end");
        assert!(out.contains("return __luao_async(function()"));
        assert!(out.contains("return 42"));
    }

    #[test]
    fn async_function_is_regular_function_outside() {
        let out = ok("async function fetch(): number return 42 end");
        assert!(out.contains("function fetch()"));
    }

    #[test]
    fn local_async_function() {
        let out = ok("local async function helper(): number return 1 end");
        assert!(out.contains("local function helper()"));
        assert!(out.contains("return __luao_async(function()"));
    }

    #[test]
    fn async_function_expression() {
        let out = ok("local f = async function(): number return 1 end");
        assert!(out.contains("__luao_async"));
    }

    // =========================================================================
    // Await emission — inside async
    // =========================================================================

    #[test]
    fn await_inside_async_uses_luao_yield() {
        let out = ok(r#"
            async function f(): number
                local x = await getValue()
                return x
            end
        "#);
        assert!(out.contains("__luao_yield(getValue())"), "await inside async → __luao_yield");
        // User code should not have :expect() calls (only the runtime's Promise:expect method definition)
        let user_code = out.split("function __luao_async(fn)").last().unwrap_or("");
        assert!(!user_code.contains("(getValue()):expect()"), "should NOT use :expect() for getValue inside async");
    }

    #[test]
    fn await_inside_async_class_method_uses_yield() {
        let out = ok(r#"
            class Svc
                async function fetch(): number
                    return await getValue()
                end
            end
        "#);
        assert!(out.contains("__luao_yield(getValue())"));
    }

    #[test]
    fn multiple_awaits_in_async() {
        let out = ok(r#"
            async function pipeline(): number
                local a = await step1()
                local b = await step2(a)
                local c = await step3(b)
                return c
            end
        "#);
        assert!(out.contains("__luao_yield(step1())"));
        assert!(out.contains("__luao_yield(step2(a))"));
        assert!(out.contains("__luao_yield(step3(b))"));
    }

    // =========================================================================
    // Top-level await emission
    // =========================================================================

    #[test]
    fn top_level_await_uses_expect() {
        let out = ok("local x = await fetchData()");
        assert!(out.contains("(fetchData()):expect()"), "top-level await → :expect()");
        // User code (after runtime) should not have __luao_yield calls
        let user_code = out.split("function __luao_async(fn)").last().unwrap_or(&out);
        assert!(!user_code.contains("__luao_yield(fetchData())"), "should NOT use __luao_yield at top level");
    }

    #[test]
    fn top_level_await_wraps_in_parens() {
        let out = ok("local x = await fetchData()");
        assert!(out.contains("(fetchData()):expect()"));
    }

    #[test]
    fn top_level_await_in_expression_statement() {
        let out = ok("await doSomething()");
        assert!(out.contains("(doSomething()):expect()"));
    }

    #[test]
    fn top_level_await_with_method_call() {
        let out = ok("local x = await obj:getData()");
        assert!(out.contains(":expect()"));
    }

    // =========================================================================
    // Nested function boundaries — in_async_context correctness
    // =========================================================================

    #[test]
    fn sync_function_inside_async_resets_context() {
        let out = ok(r#"
            async function outer(): number
                local inner = function()
                    return await somePromise()
                end
                return await other()
            end
        "#);
        // The inner sync function's await should use :expect() (not in async)
        assert!(out.contains(":expect()"), "sync fn inside async → :expect()");
        // The outer async's await should use __luao_yield
        assert!(out.contains("__luao_yield(other())"), "outer async → __luao_yield");
    }

    #[test]
    fn nested_async_inside_async() {
        let out = ok(r#"
            async function outer(): number
                local inner = async function(): number
                    return await innerPromise()
                end
                return await inner()
            end
        "#);
        // Both should use __luao_yield since both are async
        let yield_count = out.matches("__luao_yield").count();
        assert!(yield_count >= 2, "both async contexts should use __luao_yield, got {}", yield_count);
    }

    #[test]
    fn sync_method_inside_async_class_method() {
        let out = ok(r#"
            class Svc
                async function process(): number
                    local cb = function()
                        return await getValue()
                    end
                    return await other()
                end
            end
        "#);
        assert!(out.contains(":expect()"), "sync lambda inside async method → :expect()");
        assert!(out.contains("__luao_yield(other())"));
    }

    // =========================================================================
    // Promise direct usage
    // =========================================================================

    #[test]
    fn promise_new_direct() {
        let out = ok(r#"
            local p = Promise.new(function(resolve, reject)
                resolve(42)
            end)
        "#);
        assert!(out.contains("Promise.new(function(resolve, reject)"));
    }

    #[test]
    fn promise_resolve_static() {
        let out = ok("local p = Promise.resolve(42)");
        assert!(out.contains("Promise.resolve(42)"));
    }

    #[test]
    fn promise_reject_static() {
        let out = ok("local p = Promise.reject(\"oops\")");
        assert!(out.contains("Promise.reject(\"oops\")"));
    }

    #[test]
    fn promise_all_combinator() {
        let out = ok("local p = Promise.all({a(), b()})");
        assert!(out.contains("Promise.all("));
    }

    #[test]
    fn promise_race_combinator() {
        let out = ok("local p = Promise.race({a(), b()})");
        assert!(out.contains("Promise.race("));
    }

    #[test]
    fn promise_any_combinator() {
        let out = ok("local p = Promise.any({a(), b()})");
        assert!(out.contains("Promise.any("));
    }

    #[test]
    fn promise_all_settled_combinator() {
        let out = ok("local p = Promise.allSettled({a(), b()})");
        assert!(out.contains("Promise.allSettled("));
    }

    #[test]
    fn promise_try_usage() {
        let out = ok("local p = Promise.try(riskyFunction)");
        assert!(out.contains("Promise.try(riskyFunction)"));
    }

    #[test]
    fn promise_is_check() {
        let out = ok("local b = Promise.is(someVal)");
        assert!(out.contains("Promise.is(someVal)"));
    }

    #[test]
    fn promise_delay_usage() {
        let out = ok("local p = Promise.delay(5)");
        assert!(out.contains("Promise.delay(5)"));
    }

    #[test]
    fn promise_chaining_and_then() {
        let out = ok(r#"
            local p = Promise.resolve(1)
            p:andThen(function(v) print(v) end)
        "#);
        assert!(out.contains(":andThen("));
    }

    #[test]
    fn promise_chaining_catch() {
        let out = ok(r#"
            local p = Promise.reject("err")
            p:catch(function(e) print(e) end)
        "#);
        assert!(out.contains(":catch("));
    }

    #[test]
    fn promise_chaining_finally() {
        let out = ok(r#"
            local p = Promise.resolve(1)
            p:finally(function() print("done") end)
        "#);
        assert!(out.contains(":finally("));
    }

    #[test]
    fn promise_get_status() {
        let out = ok(r#"
            local p = Promise.resolve(1)
            local s = p:getStatus()
        "#);
        assert!(out.contains(":getStatus()"));
    }

    #[test]
    fn promise_cancel() {
        let out = ok(r#"
            local p = Promise.resolve(1)
            p:cancel()
        "#);
        assert!(out.contains(":cancel()"));
    }

    #[test]
    fn promise_expect_method() {
        let out = ok(r#"
            local p = Promise.resolve(1)
            local v = p:expect()
        "#);
        assert!(out.contains(":expect()"));
    }

    #[test]
    fn promise_await_method() {
        let out = ok(r#"
            local p = Promise.resolve(1)
            local s, v = p:await()
        "#);
        assert!(out.contains(":await()"));
    }

    // =========================================================================
    // Promise + async cooperation
    // =========================================================================

    #[test]
    fn async_result_chained_with_and_then() {
        let out = ok(r#"
            async function getData(): number
                return 42
            end
            getData():andThen(function(v) print(v) end)
        "#);
        assert!(out.contains("getData():andThen("));
        assert!(out.contains("__luao_async"));
    }

    #[test]
    fn await_on_promise_combinator() {
        let out = ok(r#"
            async function f(): number
                local results = await Promise.all({a(), b()})
                return results
            end
        "#);
        assert!(out.contains("__luao_yield(Promise.all("));
    }

    #[test]
    fn top_level_await_on_promise_all() {
        let out = ok("local results = await Promise.all({a(), b()})");
        assert!(out.contains("(Promise.all("));
        assert!(out.contains(")):expect()"));
    }

    #[test]
    fn instanceof_promise() {
        let out = ok(r#"
            async function f(): number return 1 end
            local p = f()
            if p instanceof Promise then
                print("yes")
            end
        "#);
        assert!(out.contains("__luao_instanceof(p, Promise)"));
    }

    #[test]
    fn promise_some_combinator() {
        let out = ok("local p = Promise.some({a(), b(), c()}, 2)");
        assert!(out.contains("Promise.some("));
    }

    // =========================================================================
    // Promise runtime content
    // =========================================================================

    #[test]
    fn promise_runtime_has_status_enum() {
        let out = ok("local p = Promise.resolve(1)");
        assert!(out.contains("Promise.Status = {"));
        assert!(out.contains("Started"));
        assert!(out.contains("Resolved"));
        assert!(out.contains("Rejected"));
        assert!(out.contains("Cancelled"));
    }

    #[test]
    fn promise_runtime_has_all_methods() {
        let out = ok("local p = Promise.resolve(1)");
        assert!(out.contains("function Promise.new(executor)"));
        assert!(out.contains("function Promise.resolve(value)"));
        assert!(out.contains("function Promise.reject(reason)"));
        assert!(out.contains("function Promise:andThen("));
        assert!(out.contains("function Promise:catch("));
        assert!(out.contains("function Promise:finally("));
        assert!(out.contains("function Promise:await()"));
        assert!(out.contains("function Promise:expect()"));
        assert!(out.contains("function Promise:cancel()"));
        assert!(out.contains("function Promise:getStatus()"));
        assert!(out.contains("function Promise.all(promises)"));
        assert!(out.contains("function Promise.race(promises)"));
        assert!(out.contains("function Promise.allSettled(promises)"));
        assert!(out.contains("function Promise.any(promises)"));
        assert!(out.contains("function Promise.some(promises, count)"));
        assert!(out.contains("function Promise.delay(seconds)"));
        assert!(out.contains("function Promise.try(fn, ...)"));
        assert!(out.contains("function Promise.is(value)"));
    }

    #[test]
    fn luao_yield_error_propagation() {
        let out = ok("async function f(): number return await g() end");
        // __luao_yield checks ok/val and errors on rejection
        assert!(out.contains("function __luao_yield(promise)"));
        assert!(out.contains("if not ok then error(val, 2) end"));
    }

    #[test]
    fn luao_async_passes_ok_flag() {
        let out = ok("async function f(): number return 1 end");
        // __luao_async step function passes true/false ok flag
        assert!(out.contains("step(true, val)"));
        assert!(out.contains("step(false, err)"));
        assert!(out.contains("step(true)"));
    }

    // =========================================================================
    // Mangling — Promise members NOT mangled by default
    // =========================================================================

    #[test]
    fn mangle_does_not_mangle_promise_members() {
        let opts = TranspileOptions { mangle: true, ..Default::default() };
        let out = ok_with("local p = Promise.resolve(42)", &opts);
        // Promise.resolve should stay as-is
        assert!(out.contains("Promise.resolve(42)"));
        assert!(out.contains("Promise.new("));
        assert!(out.contains("function Promise:andThen("));
    }

    #[test]
    fn mangle_promise_and_then_chain() {
        let opts = TranspileOptions { mangle: true, ..Default::default() };
        let out = ok_with(r#"
            local p = Promise.resolve(1)
            p:andThen(function(v) return v end)
        "#, &opts);
        assert!(out.contains(":andThen("), "andThen should not be mangled");
    }

    #[test]
    fn mangle_baseclasses_allows_mangling() {
        let opts = TranspileOptions {
            mangle: true,
            mangle_baseclasses: true,
            ..Default::default()
        };
        let out = ok_with("local p = Promise.resolve(42)", &opts);
        // With mangle_baseclasses, Promise members SHOULD be mangled
        // "resolve" should become something else
        assert!(!out.contains("Promise.resolve(42)"), "resolve should be mangled with mangle_baseclasses");
    }

    // =========================================================================
    // Checker: E022 — await context rules
    // =========================================================================

    #[test]
    fn await_in_sync_function_errors() {
        let errors = err(r#"
            function syncFn()
                local x = await someFunc()
            end
        "#);
        assert!(errors.iter().any(|e| e.contains("E022")), "should get E022 for await in sync fn");
    }

    #[test]
    fn await_in_async_function_ok() {
        let out = ok(r#"
            async function asyncFn(): number
                return await getValue()
            end
        "#);
        assert!(out.contains("__luao_yield"));
    }

    #[test]
    fn top_level_await_ok() {
        let _ = ok("local x = await fetchData()");
    }

    #[test]
    fn await_in_sync_method_errors() {
        let errors = err(r#"
            class Foo
                function bar()
                    local x = await something()
                end
            end
        "#);
        assert!(errors.iter().any(|e| e.contains("E022")));
    }

    #[test]
    fn await_in_async_method_ok() {
        let _ = ok(r#"
            class Foo
                async function bar(): number
                    return await something()
                end
            end
        "#);
    }

    #[test]
    fn await_in_nested_sync_function_inside_async_errors() {
        let errors = err(r#"
            async function outer(): number
                function inner()
                    local x = await badAwait()
                end
                return 1
            end
        "#);
        assert!(errors.iter().any(|e| e.contains("E022")), "await in nested sync fn should error");
    }

    #[test]
    fn await_in_constructor_errors() {
        let errors = err(r#"
            class Foo
                new()
                    local x = await something()
                end
            end
        "#);
        assert!(errors.iter().any(|e| e.contains("E022")));
    }

    #[test]
    fn await_in_top_level_if_ok() {
        let _ = ok(r#"
            if true then
                local x = await fetchData()
            end
        "#);
    }

    #[test]
    fn await_in_top_level_for_ok() {
        let _ = ok(r#"
            for i = 1, 3 do
                local x = await fetchItem(i)
            end
        "#);
    }

    #[test]
    fn await_in_top_level_while_ok() {
        let _ = ok(r#"
            while running do
                local x = await poll()
            end
        "#);
    }

    // =========================================================================
    // Checker: E023 — reserved name Promise
    // =========================================================================

    #[test]
    fn reserved_name_class_promise() {
        let errors = err("class Promise new() end end");
        assert!(errors.iter().any(|e| e.contains("E023")));
        assert!(errors.iter().any(|e| e.contains("reserved")));
    }

    #[test]
    fn reserved_name_interface_promise() {
        let errors = err("interface Promise end");
        assert!(errors.iter().any(|e| e.contains("E023")));
    }

    #[test]
    fn reserved_name_enum_promise() {
        let errors = err("enum Promise A end");
        assert!(errors.iter().any(|e| e.contains("E023")));
    }

    #[test]
    fn reserved_name_local_promise() {
        let errors = err("local Promise = 42");
        assert!(errors.iter().any(|e| e.contains("E023")));
    }

    #[test]
    fn reserved_name_function_promise() {
        let errors = err("function Promise() end");
        assert!(errors.iter().any(|e| e.contains("E023")));
    }

    #[test]
    fn reserved_name_type_alias_promise() {
        let errors = err("type Promise = number");
        assert!(errors.iter().any(|e| e.contains("E023")));
    }

    #[test]
    fn reserved_name_exported_class_promise() {
        let errors = err("export class Promise new() end end");
        assert!(errors.iter().any(|e| e.contains("E023")));
    }

    #[test]
    fn non_reserved_name_ok() {
        // "PromiseHelper" is NOT reserved
        let _ = ok("local PromiseHelper = 42");
    }

    #[test]
    fn non_reserved_class_ok() {
        let _ = ok("class MyPromise new() end end");
    }

    // =========================================================================
    // Generator unchanged
    // =========================================================================

    #[test]
    fn generator_still_uses_coroutine_wrap() {
        let out = ok(r#"
            generator function nums(): number
                yield 1
                yield 2
            end
        "#);
        assert!(out.contains("coroutine.wrap(function()"));
        assert!(out.contains("coroutine.yield(1)"));
        assert!(out.contains("coroutine.yield(2)"));
        assert!(!out.contains("__luao_async"), "generators should NOT use __luao_async");
    }

    #[test]
    fn generator_class_method_unchanged() {
        let out = ok(r#"
            class Seq
                generator function items(): number
                    yield 1
                end
            end
        "#);
        assert!(out.contains("coroutine.wrap(function()"));
    }

    // =========================================================================
    // Edge cases
    // =========================================================================

    #[test]
    fn async_function_no_await() {
        let out = ok("async function f(): number return 42 end");
        assert!(out.contains("__luao_async(function()"));
    }

    #[test]
    fn empty_async_function() {
        let out = ok("async function f() end");
        assert!(out.contains("__luao_async(function()"));
    }

    #[test]
    fn promise_used_without_async() {
        // Promise used directly, no async keyword anywhere
        let out = ok(r#"
            local p = Promise.new(function(resolve, reject)
                resolve(42)
            end)
            local s = p:getStatus()
        "#);
        assert!(out.contains("local Promise = {}"), "should include Promise runtime");
        // User code portion should not have __luao_yield
        let user_code = out.split("function __luao_async(fn)").last().unwrap_or(&out);
        assert!(!user_code.contains("__luao_yield("), "no await used in user code");
    }

    #[test]
    fn await_complex_expression() {
        let out = ok(r#"
            async function f(): number
                local x = await getMap():get("key")
                return x
            end
        "#);
        assert!(out.contains("__luao_yield("));
    }

    #[test]
    fn multiple_top_level_awaits() {
        let out = ok(r#"
            local a = await fetchA()
            local b = await fetchB()
            local c = await fetchC()
        "#);
        // Check user code portion only (after the runtime's __luao_async definition)
        let user_code = out.split("function __luao_async(fn)").last().unwrap_or(&out);
        let expect_count = user_code.matches(":expect()").count();
        assert_eq!(expect_count, 3, "should have 3 :expect() calls in user code, got {}", expect_count);
    }

    #[test]
    fn async_class_method_with_self() {
        let out = ok(r#"
            class DataService
                private url: string
                new(url: string)
                    self.url = url
                end
                async function fetch(): string
                    local data = await getData(self.url)
                    return data
                end
            end
        "#);
        assert!(out.contains("__luao_async(function()"));
        assert!(out.contains("__luao_yield(getData(self.url))"));
    }

    #[test]
    fn promise_status_field_accessible() {
        let out = ok("local s = Promise.Status");
        assert!(out.contains("Promise.Status"));
    }

    #[test]
    fn instanceof_promise_passes_checker() {
        // Should not produce E019 (instanceof RHS must be class name)
        let _ = ok(r#"
            local p = Promise.resolve(1)
            local b = p instanceof Promise
        "#);
    }

    #[test]
    fn async_returning_promise_chain() {
        let out = ok(r#"
            async function f(): number
                return await Promise.resolve(42)
            end
        "#);
        assert!(out.contains("__luao_yield(Promise.resolve(42))"));
    }

    #[test]
    fn chained_await_in_async() {
        let out = ok(r#"
            async function f(): number
                local p = await Promise.all({a(), b()})
                local q = await Promise.race({c(), d()})
                return p
            end
        "#);
        assert!(out.contains("__luao_yield(Promise.all("));
        assert!(out.contains("__luao_yield(Promise.race("));
    }
}
