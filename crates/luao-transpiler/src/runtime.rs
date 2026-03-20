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
