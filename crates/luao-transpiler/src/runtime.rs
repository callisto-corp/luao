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

pub const PROMISE_RUNTIME: &str = r#"local Promise = {}
Promise.__index = Promise

Promise.Status = { Started = "Started", Resolved = "Resolved", Rejected = "Rejected", Cancelled = "Cancelled" }

function Promise.new(executor)
    local self = setmetatable({
        _status = "Started",
        _value = nil,
        _callbacks = {},
        _cancelHook = nil,
    }, Promise)

    local function resolve(value)
        if self._status ~= "Started" then return end
        self._status = "Resolved"
        self._value = value
        for _, cb in ipairs(self._callbacks) do cb(self._status, value) end
    end

    local function reject(reason)
        if self._status ~= "Started" then return end
        self._status = "Rejected"
        self._value = reason
        for _, cb in ipairs(self._callbacks) do cb(self._status, reason) end
    end

    local function onCancel(hook)
        self._cancelHook = hook
    end

    local ok, err = pcall(executor, resolve, reject, onCancel)
    if not ok and self._status == "Started" then
        reject(err)
    end

    return self
end

function Promise.resolve(value)
    return Promise.new(function(resolve) resolve(value) end)
end

function Promise.reject(reason)
    return Promise.new(function(_, reject) reject(reason) end)
end

function Promise:andThen(onFulfilled, onRejected)
    return Promise.new(function(resolve, reject)
        local function handle(status, value)
            if status == "Resolved" then
                if onFulfilled then
                    local ok, result = pcall(onFulfilled, value)
                    if ok then
                        if type(result) == "table" and getmetatable(result) == Promise then
                            result:andThen(resolve, reject)
                        else
                            resolve(result)
                        end
                    else
                        reject(result)
                    end
                else
                    resolve(value)
                end
            elseif status == "Rejected" then
                if onRejected then
                    local ok, result = pcall(onRejected, value)
                    if ok then
                        resolve(result)
                    else
                        reject(result)
                    end
                else
                    reject(value)
                end
            end
        end
        if self._status == "Resolved" or self._status == "Rejected" then
            handle(self._status, self._value)
        else
            self._callbacks[#self._callbacks + 1] = handle
        end
    end)
end

function Promise:catch(onRejected)
    return self:andThen(nil, onRejected)
end

function Promise:finally(callback)
    return self:andThen(function(value)
        callback()
        return value
    end, function(reason)
        callback()
        error(reason)
    end)
end

function Promise:await()
    if self._status == "Resolved" then
        return self._status, self._value
    elseif self._status == "Rejected" then
        return self._status, self._value
    end
    local co = coroutine.running()
    if co then
        self:andThen(function(value)
            coroutine.resume(co, "Resolved", value)
        end, function(reason)
            coroutine.resume(co, "Rejected", reason)
        end)
        return coroutine.yield()
    end
    return self._status, self._value
end

function Promise:expect()
    local status, value = self:await()
    if status == "Rejected" then
        error(value, 2)
    end
    return value
end

function Promise:cancel()
    if self._status ~= "Started" then return end
    self._status = "Cancelled"
    if self._cancelHook then
        self._cancelHook()
    end
    for _, cb in ipairs(self._callbacks) do cb(self._status, nil) end
end

function Promise:getStatus()
    return self._status
end

function Promise.all(promises)
    return Promise.new(function(resolve, reject)
        local results = {}
        local remaining = #promises
        if remaining == 0 then resolve(results) return end
        for i, p in ipairs(promises) do
            p:andThen(function(value)
                results[i] = value
                remaining = remaining - 1
                if remaining == 0 then resolve(results) end
            end, function(reason)
                reject(reason)
            end)
        end
    end)
end

function Promise.race(promises)
    return Promise.new(function(resolve, reject)
        for _, p in ipairs(promises) do
            p:andThen(resolve, reject)
        end
    end)
end

function Promise.allSettled(promises)
    return Promise.new(function(resolve)
        local results = {}
        local remaining = #promises
        if remaining == 0 then resolve(results) return end
        for i, p in ipairs(promises) do
            p:andThen(function(value)
                results[i] = { status = "Resolved", value = value }
                remaining = remaining - 1
                if remaining == 0 then resolve(results) end
            end, function(reason)
                results[i] = { status = "Rejected", reason = reason }
                remaining = remaining - 1
                if remaining == 0 then resolve(results) end
            end)
        end
    end)
end

function Promise.any(promises)
    return Promise.new(function(resolve, reject)
        local errors = {}
        local remaining = #promises
        if remaining == 0 then reject("All promises were rejected") return end
        for i, p in ipairs(promises) do
            p:andThen(function(value)
                resolve(value)
            end, function(reason)
                errors[i] = reason
                remaining = remaining - 1
                if remaining == 0 then reject(errors) end
            end)
        end
    end)
end

function Promise.some(promises, count)
    return Promise.new(function(resolve, reject)
        local results = {}
        local errors = {}
        local resolved = 0
        local rejected = 0
        local total = #promises
        if total == 0 or count <= 0 then resolve(results) return end
        for i, p in ipairs(promises) do
            p:andThen(function(value)
                if resolved < count then
                    results[#results + 1] = value
                    resolved = resolved + 1
                    if resolved >= count then resolve(results) end
                end
            end, function(reason)
                errors[i] = reason
                rejected = rejected + 1
                if rejected > total - count then reject(errors) end
            end)
        end
    end)
end

function Promise.delay(seconds)
    return Promise.new(function(resolve, _, onCancel)
        local cancelled = false
        onCancel(function() cancelled = true end)
        task.delay(seconds, function()
            if not cancelled then resolve() end
        end)
    end)
end

function Promise.try(fn, ...)
    local args = {...}
    return Promise.new(function(resolve, reject)
        local ok, result = pcall(fn, table.unpack(args))
        if ok then
            if type(result) == "table" and getmetatable(result) == Promise then
                result:andThen(resolve, reject)
            else
                resolve(result)
            end
        else
            reject(result)
        end
    end)
end

function Promise.is(value)
    return type(value) == "table" and getmetatable(value) == Promise
end

function __luao_yield(promise)
    local ok, val = coroutine.yield(promise)
    if not ok then error(val, 2) end
    return val
end

function __luao_async(fn)
    return Promise.new(function(resolve, reject)
        local co = coroutine.create(fn)
        local function step(ok, ...)
            local results = {coroutine.resume(co, ok, ...)}
            local resumed = results[1]
            if not resumed then
                reject(results[2])
                return
            end
            if coroutine.status(co) == "dead" then
                resolve(results[2])
                return
            end
            local yielded = results[2]
            if Promise.is(yielded) then
                yielded:andThen(function(val)
                    step(true, val)
                end, function(err)
                    step(false, err)
                end)
            else
                step(true, yielded)
            end
        end
        step(true)
    end)
end"#;

pub const ARRAY_RUNTIME: &str = r##"local __luao_Array
do
    local ArrayMT = {}
    ArrayMT.__index = ArrayMT

    function ArrayMT:push(...)
        local args = {...}
        for i = 1, #args do
            self[#self + 1] = args[i]
        end
        return #self
    end

    function ArrayMT:pop()
        return table.remove(self)
    end

    function ArrayMT:shift()
        return table.remove(self, 1)
    end

    function ArrayMT:unshift(value)
        table.insert(self, 1, value)
        return #self
    end

    function ArrayMT:reverse()
        local n = #self
        for i = 1, math.floor(n / 2) do
            self[i], self[n - i + 1] = self[n - i + 1], self[i]
        end
        return self
    end

    function ArrayMT:sort(cmp)
        table.sort(self, cmp)
        return self
    end

    function ArrayMT:map(fn)
        local result = {}
        for i = 1, #self do
            result[i] = fn(self[i], i)
        end
        return setmetatable(result, ArrayMT)
    end

    function ArrayMT:filter(fn)
        local result = {}
        for i = 1, #self do
            if fn(self[i], i) then
                result[#result + 1] = self[i]
            end
        end
        return setmetatable(result, ArrayMT)
    end

    function ArrayMT:reduce(fn, init)
        local acc = init
        local start = 1
        if acc == nil then
            acc = self[1]
            start = 2
        end
        for i = start, #self do
            acc = fn(acc, self[i], i)
        end
        return acc
    end

    function ArrayMT:reduceRight(fn, init)
        local acc = init
        local start = #self
        if acc == nil then
            acc = self[#self]
            start = #self - 1
        end
        for i = start, 1, -1 do
            acc = fn(acc, self[i], i)
        end
        return acc
    end

    function ArrayMT:find(fn)
        for i = 1, #self do
            if fn(self[i], i) then return self[i] end
        end
        return nil
    end

    function ArrayMT:findIndex(fn)
        for i = 1, #self do
            if fn(self[i], i) then return i end
        end
        return -1
    end

    function ArrayMT:indexOf(value)
        for i = 1, #self do
            if self[i] == value then return i end
        end
        return -1
    end

    function ArrayMT:lastIndexOf(value)
        for i = #self, 1, -1 do
            if self[i] == value then return i end
        end
        return -1
    end

    function ArrayMT:includes(value)
        for i = 1, #self do
            if self[i] == value then return true end
        end
        return false
    end

    function ArrayMT:every(fn)
        for i = 1, #self do
            if not fn(self[i], i) then return false end
        end
        return true
    end

    function ArrayMT:some(fn)
        for i = 1, #self do
            if fn(self[i], i) then return true end
        end
        return false
    end

    function ArrayMT:forEach(fn)
        for i = 1, #self do
            fn(self[i], i)
        end
    end

    function ArrayMT:slice(start, stop)
        start = start or 1
        stop = stop or #self
        if start < 0 then start = #self + start + 1 end
        if stop < 0 then stop = #self + stop + 1 end
        local result = {}
        for i = start, stop do
            result[#result + 1] = self[i]
        end
        return setmetatable(result, ArrayMT)
    end

    function ArrayMT:concat(other)
        local result = {}
        for i = 1, #self do result[#result + 1] = self[i] end
        for i = 1, #other do result[#result + 1] = other[i] end
        return setmetatable(result, ArrayMT)
    end

    function ArrayMT:flat()
        local result = {}
        for i = 1, #self do
            local v = self[i]
            if type(v) == "table" then
                for j = 1, #v do
                    result[#result + 1] = v[j]
                end
            else
                result[#result + 1] = v
            end
        end
        return setmetatable(result, ArrayMT)
    end

    function ArrayMT:flatMap(fn)
        return self:map(fn):flat()
    end

    function ArrayMT:join(sep)
        sep = sep or ","
        local parts = {}
        for i = 1, #self do
            parts[i] = tostring(self[i])
        end
        return table.concat(parts, sep)
    end

    function ArrayMT:unpack()
        return table.unpack(self)
    end

    function ArrayMT:values()
        local i = 0
        return function()
            i = i + 1
            if i <= #self then return self[i] end
        end
    end

    function ArrayMT:entries()
        local i = 0
        return function()
            i = i + 1
            if i <= #self then return i, self[i] end
        end
    end

    function ArrayMT:keys()
        local i = 0
        return function()
            i = i + 1
            if i <= #self then return i end
        end
    end

    function ArrayMT:__len()
        return rawlen(self)
    end

    function ArrayMT:__tostring()
        local parts = {}
        for i = 1, rawlen(self) do
            local v = rawget(self, i)
            if type(v) == "string" then
                parts[i] = '"' .. v .. '"'
            else
                parts[i] = tostring(v)
            end
        end
        return "[" .. table.concat(parts, ", ") .. "]"
    end

    function ArrayMT:__concat(other)
        if getmetatable(self) == ArrayMT and getmetatable(other) == ArrayMT then
            return self:concat(other)
        end
        return tostring(self) .. tostring(other)
    end

    __luao_Array = function(t)
        if t then
            return setmetatable(t, ArrayMT)
        end
        return setmetatable({}, ArrayMT)
    end

    Array = setmetatable({
        of = function(...)
            return setmetatable({...}, ArrayMT)
        end,
        from = function(t)
            local arr = {}
            for i = 1, #t do arr[i] = t[i] end
            return setmetatable(arr, ArrayMT)
        end,
        isArray = function(v)
            return type(v) == "table" and getmetatable(v) == ArrayMT
        end,
        range = function(start, stop, step)
            step = step or 1
            local arr = {}
            local idx = 1
            for i = start, stop, step do
                arr[idx] = i
                idx = idx + 1
            end
            return setmetatable(arr, ArrayMT)
        end,
    }, {
        __call = function(_, t) return __luao_Array(t) end,
    })
end"##;
