pub const INSTANCEOF_FN: &str = r#"function __luao_instanceof(obj, class)
    local mt = getmetatable(obj)
    while mt do
        if mt == class then return true end
        local parent = getmetatable(mt)
        if parent then mt = parent.__index else mt = nil end
    end
    return false
end"#;

pub const ENUM_FREEZE_FN: &str = r#"function __luao_enum_freeze(t)
    setmetatable(t, { __newindex = function() error("Cannot modify enum") end })
end"#;

pub const ABSTRACT_GUARD_FN: &str = r#"function __luao_abstract_guard(self, class, className)
    if getmetatable(self) == class then
        error("Cannot instantiate abstract class '" .. className .. "'", 2)
    end
end"#;

pub const ASYNC_RUNTIME: &str = r#"function __luao_async(fn)
    local task = {
        _callbacks = {},
        _status = "pending",
    }
    task._co = coroutine.create(fn)
    local function finish(ok, result)
        if ok then
            task._status = "resolved"
            task._result = result
        else
            task._status = "rejected"
            task._error = result
        end
        for i = 1, #task._callbacks do
            task._callbacks[i](task._result, task._error)
        end
    end
    local function step(value)
        local ok, yielded = coroutine.resume(task._co, value)
        if not ok then
            finish(false, yielded)
            return
        end
        if coroutine.status(task._co) == "dead" then
            finish(true, yielded)
        elseif type(yielded) == "table" and yielded._status ~= nil then
            if yielded._status ~= "pending" then
                step(yielded._result)
            else
                yielded:andThen(function(result, err)
                    if err then finish(false, err) else step(result) end
                end)
            end
        else
            step(yielded)
        end
    end
    function task:andThen(cb)
        if self._status ~= "pending" then
            cb(self._result, self._error)
        else
            self._callbacks[#self._callbacks + 1] = cb
        end
        return self
    end
    function task:await()
        while self._status == "pending" do
            coroutine.yield()
        end
        if self._error then error(self._error) end
        return self._result
    end
    step()
    return task
end

function __luao_await(value)
    return coroutine.yield(value)
end"#;
