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
