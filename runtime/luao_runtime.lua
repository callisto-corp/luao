function __luao_instanceof(obj, class)
    local mt = getmetatable(obj)
    while mt do
        if mt == class then
            return true
        end
        local parent = getmetatable(mt)
        if parent then
            mt = parent.__index
        else
            mt = nil
        end
    end
    return false
end

function __luao_enum_freeze(t)
    setmetatable(t, {
        __newindex = function()
            error("Cannot modify enum")
        end
    })
end

function __luao_abstract_guard(name)
    error("Cannot instantiate abstract class " .. name)
end
