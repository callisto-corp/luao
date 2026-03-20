# Luao Language Specification

**Version:** 0.1.0
**Status:** Draft
**Date:** 2026-03-18

---

## Table of Contents

1. [Overview](#1-overview)
2. [Classes](#2-classes)
3. [Inheritance](#3-inheritance)
4. [The `super` Keyword](#4-the-super-keyword)
5. [Interfaces](#5-interfaces)
6. [Access Modifiers](#6-access-modifiers)
7. [Static Members](#7-static-members)
8. [Abstract Classes](#8-abstract-classes)
9. [Sealed Classes](#9-sealed-classes)
10. [Readonly Fields](#10-readonly-fields)
11. [Operator Overloading](#11-operator-overloading)
12. [Enums](#12-enums)
13. [Property Getters and Setters](#13-property-getters-and-setters)
14. [Type Annotations](#14-type-annotations)
15. [The `override` Keyword](#15-the-override-keyword)
16. [The `instanceof` Operator](#16-the-instanceof-operator)
17. [Generators](#17-generators)
18. [Async and Await](#18-async-and-await)
19. [Grammar (EBNF)](#19-grammar-ebnf)
20. [Transpilation Reference](#20-transpilation-reference)
21. [Runtime Library](#21-runtime-library)

---

## 1. Overview

Luao is a **strict superset of Lua 5.4** that adds object-oriented programming features. Every valid Lua 5.4 program is a valid Luao program with identical semantics. Luao extends Lua with classes, interfaces, enums, type annotations, and other constructs commonly found in object-oriented languages.

### Design Principles

- **Full backward compatibility.** Any valid Lua 5.4 source file is accepted by the Luao compiler without modification.
- **Zero runtime overhead by default.** Type annotations and interfaces are erased at transpile time. Only features that require runtime behavior (classes, enums, `instanceof`) emit code.
- **Transparent output.** Luao transpiles to idiomatic Lua 5.4 using tables and metatables. The generated code is human-readable and debuggable.
- **Gradual adoption.** Luao features can be introduced incrementally into an existing Lua codebase.

### File Extension

Luao source files use the `.luao` extension. The transpiler produces `.lua` output files.

### Compilation Pipeline

```
.luao source
    -> Lexer (tokenization)
    -> Parser (AST construction)
    -> Resolver (name resolution)
    -> Checker (type checking, access control, sealed enforcement)
    -> Transpiler (Lua 5.4 code generation)
    -> .lua output
```

### Notation Conventions

Throughout this specification:

- `LUAO` refers to Luao source code (input).
- `LUA` refers to the generated Lua 5.4 code (output).
- Grammar productions use EBNF notation as defined in [Section 19](#19-grammar-ebnf).

---

## 2. Classes

A class declaration introduces a named type that transpiles to a Lua table acting as both a constructor factory and a metatable for its instances.

### Syntax

```
class Name
    -- field declarations
    -- constructor
    -- method declarations
end
```

### Field Declarations

Fields are declared inside the class body with an optional access modifier, an optional `readonly` qualifier, a name, an optional type annotation, and an optional default value.

```
class Person
    public name: string
    private age: number = 0
    protected id: string
end
```

If no access modifier is provided, fields default to `public`.

Field default values are assigned in the constructor output before the user-defined constructor body executes.

### Constructor

Each class may declare at most one constructor using the `new` keyword. The constructor defines how instances are created.

```
class Person
    public name: string
    private age: number

    new(name: string, age: number)
        self.name = name
        self.age = age
    end
end
```

If no constructor is declared, a default no-argument constructor is generated that creates an empty instance with any declared field defaults applied.

Inside the constructor body, `self` refers to the newly created instance. The constructor must not explicitly return a value; the transpiler handles returning `self`.

### Usage

```
local p = Person.new("Alice", 30)
print(p.name) -- "Alice"
```

### Methods

Methods are functions declared inside the class body. They receive `self` as an implicit first argument (via Lua's `:` calling convention) unless declared `static`.

```
class Person
    public name: string

    new(name: string)
        self.name = name
    end

    public function greet(): string
        return "Hello, I am " .. self.name
    end
end
```

Methods are invoked with `:` syntax:

```
local p = Person.new("Alice")
print(p:greet()) -- "Hello, I am Alice"
```

### Transpilation

A class transpiles to a Lua table that serves as both the class object and the metatable for instances. See [Section 20](#20-transpilation-reference) for full examples.

The general pattern is:

```lua
-- LUA output
local Person = {}
Person.__index = Person

function Person.new(name, age)
    local self = setmetatable({}, Person)
    self.name = name
    self.age = age
    return self
end

function Person:greet()
    return "Hello, I am " .. self.name
end
```

---

## 3. Inheritance

Luao supports **single inheritance** via the `extends` keyword. A class may extend at most one parent class.

### Syntax

```
class Dog extends Animal
    -- fields, constructor, methods
end
```

### Semantics

When class `B extends A`:

1. `B` gains access to all `public` and `protected` members of `A`.
2. `B.__index` is set up to fall through to `A` via a metatable chain, so that instances of `B` can access methods defined on `A` when not overridden.
3. `B` instances are considered instances of both `B` and `A` for the purposes of `instanceof`.

### Metatable Chain

The inheritance chain is implemented by setting a metatable on the child class table itself:

```lua
-- LUA output
local Animal = {}
Animal.__index = Animal

local Dog = {}
Dog.__index = Dog
setmetatable(Dog, { __index = Animal })
```

When a method is looked up on a `Dog` instance:

1. Look in the instance table.
2. Look in `Dog` (the instance's metatable via `__index`).
3. Look in `Animal` (Dog's metatable's `__index`).

### Constructor Chaining

A child class constructor should call the parent constructor using `super.new(args)` to ensure parent initialization runs. This is not enforced syntactically but will produce a diagnostic if the parent has a constructor and the child does not call it.

```
class Animal
    public name: string

    new(name: string)
        self.name = name
    end
end

class Dog extends Animal
    public breed: string

    new(name: string, breed: string)
        super.new(name)
        self.breed = breed
    end
end
```

### Multiple Inheritance

Multiple inheritance is **not supported**. A class may extend at most one other class. To share behavior across unrelated classes, use interfaces (compile-time contracts) or composition.

---

## 4. The `super` Keyword

The `super` keyword provides access to the parent class from within a child class. It is only valid inside a class body that uses `extends`.

### `super.new(args)`

Calls the parent class constructor with the given arguments, passing the current `self` as the receiver.

```
class Dog extends Animal
    new(name: string, breed: string)
        super.new(name) -- calls Animal.new(self, name)
        self.breed = breed
    end
end
```

**Transpilation:**

```lua
function Dog.new(name, breed)
    local self = setmetatable({}, Dog)
    Animal.new(self, name) -- super.new(name)
    self.breed = breed
    return self
end
```

Note that `super.new` in a child constructor does **not** create a new object. It calls the parent constructor as a plain function, passing the already-created `self`.

### `super.method(args)`

Calls a parent class method explicitly, bypassing any override in the current class. Useful when a child method wants to extend rather than replace parent behavior.

```
class Dog extends Animal
    override function speak(): string
        local base = super.speak()
        return base .. " (but louder)"
    end
end
```

**Transpilation:**

```lua
function Dog:speak()
    local base = Animal.speak(self) -- super.speak()
    return base .. " (but louder)"
end
```

### Restrictions

- `super` is only valid inside a class that has a parent class (`extends`). Using `super` in a non-extending class is a compile error.
- `super` may not be used outside of a class body.
- `super` may not be stored in a variable or passed as a value; it is a keyword, not an expression.

---

## 5. Interfaces

Interfaces define contracts that classes must fulfill. They describe method signatures and, optionally, field shapes without providing implementations.

### Syntax

```
interface Drawable
    function draw(x: number, y: number): void
    function getLayer(): number
end
```

### Semantics

- Interfaces are **compile-time only**. They produce no runtime code.
- A class declares that it fulfills an interface with the `implements` keyword.
- A class may implement multiple interfaces.
- The checker verifies at compile time that the class provides all methods and fields declared by each interface it implements.

### Implementing Interfaces

```
class Circle implements Drawable
    public radius: number

    new(radius: number)
        self.radius = radius
    end

    public function draw(x: number, y: number): void
        -- drawing logic
    end

    public function getLayer(): number
        return 1
    end
end
```

If `Circle` omitted `draw` or `getLayer`, the checker would emit a compile error listing the missing members.

### Multiple Interfaces

```
class Button implements Drawable, Clickable
    -- must implement all methods from both Drawable and Clickable
end
```

### Interface Extension

An interface may extend one or more other interfaces, inheriting their declarations.

```
interface Shape
    function area(): number
end

interface Drawable
    function draw(x: number, y: number): void
end

interface DrawableShape extends Shape, Drawable
    function boundingBox(): Table
end
```

A class implementing `DrawableShape` must provide `area`, `draw`, and `boundingBox`.

### Interface Fields

Interfaces may declare fields that implementing classes must have:

```
interface Named
    name: string
end
```

### Combined `extends` and `implements`

A class may both extend a parent class and implement interfaces:

```
class Dog extends Animal implements Named, Serializable
    -- ...
end
```

The `extends` clause must appear before `implements`.

---

## 6. Access Modifiers

Access modifiers control the visibility of class members (fields and methods).

### Modifiers

| Modifier    | Accessible from                                  |
|-------------|--------------------------------------------------|
| `public`    | Anywhere                                         |
| `private`   | Only within the declaring class                  |
| `protected` | Within the declaring class and its subclasses    |

### Defaults

- **Methods** default to `public` if no modifier is specified.
- **Fields** default to `public` if no modifier is specified.

### Syntax

```
class Account
    public owner: string
    private balance: number
    protected accountType: string

    new(owner: string, balance: number)
        self.owner = owner
        self.balance = balance
        self.accountType = "standard"
    end

    public function getBalance(): number
        return self.balance
    end

    private function validate(): boolean
        return self.balance >= 0
    end

    protected function adjustBalance(amount: number): void
        self.balance = self.balance + amount
    end
end
```

### Enforcement

Access modifiers are enforced at **compile time** by the checker. There is no runtime access control.

- Accessing a `private` member from outside the declaring class produces a compile error.
- Accessing a `protected` member from a class that is not the declaring class or a subclass produces a compile error.

### Transpilation of Private Fields

Private fields are emitted with a `_` prefix in the Lua output to signal their intended visibility, but this is purely conventional. There is no runtime enforcement.

```
-- LUAO
class Foo
    private secret: string
end

-- LUA output
-- self._secret accessed only within Foo methods
```

### Transpilation of Private and Protected Methods

Private and protected methods are transpiled as regular methods on the class table. The access restriction is enforced only at compile time.

---

## 7. Static Members

Static members belong to the class table itself rather than to instances. They are shared across all instances and can be accessed without creating an instance.

### Static Methods

```
class MathUtils
    static function add(a: number, b: number): number
        return a + b
    end

    static function clamp(value: number, min: number, max: number): number
        if value < min then return min end
        if value > max then return max end
        return value
    end
end
```

Static methods:

- Do **not** receive an implicit `self` parameter.
- Are called with `.` (dot) syntax, not `:` (colon) syntax.
- May not reference `self` in their body.

```
local sum = MathUtils.add(2, 3)  -- 5
```

### Static Fields

```
class Config
    static VERSION: string = "1.0.0"
    static MAX_RETRIES: number = 3
end
```

Static fields are stored directly on the class table:

```
print(Config.VERSION) -- "1.0.0"
```

### Transpilation

```lua
-- LUA output
local MathUtils = {}
MathUtils.__index = MathUtils

function MathUtils.add(a, b)  -- dot, no self
    return a + b
end

Config.VERSION = "1.0.0"
Config.MAX_RETRIES = 3
```

### Static Members and Inheritance

Static members are **not inherited** by subclasses via the metatable chain by default. They belong to the declaring class table only. If access through a subclass is desired, the metatable chain on the class tables provides fallback lookup (since `setmetatable(Child, { __index = Parent })` is applied).

---

## 8. Abstract Classes

An abstract class cannot be instantiated directly. It serves as a base class that defines a contract for its subclasses.

### Syntax

```
abstract class Shape
    public color: string

    new(color: string)
        self.color = color
    end

    abstract function area(): number
    abstract function perimeter(): number

    public function describe(): string
        return self.color .. " shape with area " .. tostring(self:area())
    end
end
```

### Abstract Methods

- Declared with the `abstract` keyword.
- Have **no body** (no code between the signature and `end`; there is no `end` for abstract methods).
- Must be implemented by any non-abstract subclass.

### Rules

1. A class with one or more `abstract` methods must itself be declared `abstract`.
2. Instantiating an abstract class directly is a compile error.
3. A subclass that does not implement all inherited abstract methods must itself be declared `abstract`.
4. Abstract classes **may** have concrete (non-abstract) methods and fields.

### Concrete Subclass

```
class Circle extends Shape
    public radius: number

    new(color: string, radius: number)
        super.new(color)
        self.radius = radius
    end

    override function area(): number
        return math.pi * self.radius ^ 2
    end

    override function perimeter(): number
        return 2 * math.pi * self.radius
    end
end

local c = Circle.new("red", 5)
print(c:area())      -- 78.539...
print(c:describe())  -- "red shape with area 78.539..."
```

### Runtime Guard

At runtime, the abstract class constructor includes a guard that prevents direct instantiation. This ensures that even if the compile-time check is bypassed (e.g., by calling from plain Lua code), an error is raised.

```lua
-- LUA output
function Shape.new(color)
    local self = setmetatable({}, Shape)
    __luao_abstract_guard(self, Shape, "Shape")
    self.color = color
    return self
end
```

See [Section 21](#21-runtime-library) for the definition of `__luao_abstract_guard`.

---

## 9. Sealed Classes

A sealed class cannot be extended by classes defined outside the same source file.

### Syntax

```
sealed class Config
    public debug: boolean
    public logLevel: string

    new(debug: boolean, logLevel: string)
        self.debug = debug
        self.logLevel = logLevel
    end
end
```

### Semantics

- Within the same `.luao` file, a sealed class may be extended normally.
- In any other file, writing `class Foo extends Config` is a compile error.
- This restriction is enforced entirely at compile time. No runtime code is emitted for sealed enforcement.

### Use Cases

Sealed classes are useful when:

- A class hierarchy is intentionally closed (e.g., a fixed set of node types in an AST).
- Internal implementation details should not leak through extension.

### Combined Modifiers

A class may be both `abstract` and `sealed`:

```
abstract sealed class Expr
    abstract function eval(): number
end

-- In the same file:
class LiteralExpr extends Expr
    public value: number
    new(value: number)
        self.value = value
    end
    override function eval(): number
        return self.value
    end
end

class BinOpExpr extends Expr
    -- ...
end
```

This pattern creates a closed hierarchy: only the variants in the same file are permitted.

---

## 10. Readonly Fields

A readonly field may only be assigned inside the constructor. Any assignment to it outside the constructor is a compile error.

### Syntax

```
class User
    readonly id: number
    public name: string

    new(id: number, name: string)
        self.id = id        -- OK: inside constructor
        self.name = name
    end

    public function changeName(newName: string): void
        self.name = newName  -- OK: name is not readonly
    end

    public function resetId(newId: number): void
        self.id = newId      -- COMPILE ERROR: id is readonly
    end
end
```

### Semantics

- The `readonly` qualifier may appear alongside an access modifier: `public readonly id: number`, `private readonly key: string`.
- Readonly is enforced at **compile time** only. The transpiled Lua output has no runtime guard for readonly fields (the field is a normal table entry).
- Readonly fields must be assigned a value in the constructor or have a default value in their declaration. A readonly field that is never assigned produces a warning.

### Transpilation

Readonly fields transpile identically to regular fields. The constraint is purely a compile-time check.

---

## 11. Operator Overloading

Luao allows classes to define metamethods as regular methods using Lua's standard metamethod names. These methods are placed on the class table, which serves as the metatable for instances.

### Supported Metamethods

| Method         | Lua Metamethod | Operation             |
|----------------|----------------|-----------------------|
| `__add`        | `__add`        | `a + b`               |
| `__sub`        | `__sub`        | `a - b`               |
| `__mul`        | `__mul`        | `a * b`               |
| `__div`        | `__div`        | `a / b`               |
| `__mod`        | `__mod`        | `a % b`               |
| `__pow`        | `__pow`        | `a ^ b`               |
| `__unm`        | `__unm`        | `-a`                  |
| `__idiv`       | `__idiv`       | `a // b`              |
| `__band`       | `__band`       | `a & b`               |
| `__bor`        | `__bor`        | `a \| b`              |
| `__bxor`       | `__bxor`       | `a ~ b`               |
| `__bnot`       | `__bnot`       | `~a`                  |
| `__shl`        | `__shl`        | `a << b`              |
| `__shr`        | `__shr`        | `a >> b`              |
| `__eq`         | `__eq`         | `a == b`              |
| `__lt`         | `__lt`         | `a < b`               |
| `__le`         | `__le`         | `a <= b`              |
| `__len`        | `__len`        | `#a`                  |
| `__concat`     | `__concat`     | `a .. b`              |
| `__call`       | `__call`       | `a(args)`             |
| `__tostring`   | `__tostring`   | `tostring(a)`         |

### Syntax

Metamethods are declared as regular class methods:

```
class Vector
    public x: number
    public y: number

    new(x: number, y: number)
        self.x = x
        self.y = y
    end

    function __add(other: Vector): Vector
        return Vector.new(self.x + other.x, self.y + other.y)
    end

    function __tostring(): string
        return "(" .. self.x .. ", " .. self.y .. ")"
    end

    function __eq(other: Vector): boolean
        return self.x == other.x and self.y == other.y
    end

    function __len(): number
        return math.sqrt(self.x ^ 2 + self.y ^ 2)
    end
end
```

### Usage

```
local a = Vector.new(1, 2)
local b = Vector.new(3, 4)
local c = a + b          -- calls __add
print(tostring(c))       -- "(4, 6)"
print(a == b)            -- false
print(#a)                -- 2.2360...
```

### Transpilation

Because the class table **is** the metatable for instances (via `setmetatable({}, ClassName)`), any method named with a `__` prefix automatically acts as a metamethod. No special transpilation is needed beyond the standard method definition.

```lua
-- LUA output
function Vector:__add(other)
    return Vector.new(self.x + other.x, self.y + other.y)
end

function Vector:__tostring()
    return "(" .. self.x .. ", " .. self.y .. ")"
end
```

---

## 12. Enums

Enums define a fixed set of named constants.

### Syntax

```
enum Direction
    North = 1
    South = 2
    East = 3
    West = 4
end
```

### Auto-Increment

If values are omitted, they auto-increment starting from `1`:

```
enum Color
    Red        -- 1
    Green      -- 2
    Blue       -- 3
end
```

Auto-increment applies only to entries that omit an explicit value. If an entry specifies a numeric value, subsequent omitted entries increment from that value:

```
enum HttpStatus
    OK = 200
    Created        -- 201
    Accepted       -- 202
    BadRequest = 400
    Unauthorized   -- 401
end
```

### String Values

Enum values may also be strings:

```
enum LogLevel
    Debug = "debug"
    Info = "info"
    Warn = "warn"
    Error = "error"
end
```

When string values are used, auto-increment is not available; every entry must have an explicit value.

### Usage

```
local dir = Direction.North
print(dir)  -- 1

-- Reverse lookup
print(Direction._values[1])  -- "North"
```

### Immutability

Enum tables are frozen at runtime. Attempting to assign a new key or modify an existing key raises an error:

```
Direction.North = 99  -- RUNTIME ERROR: attempt to modify frozen enum 'Direction'
```

### Transpilation

```lua
-- LUA output
local Direction = __luao_enum_freeze("Direction", {
    North = 1,
    South = 2,
    East = 3,
    West = 4,
}, {
    [1] = "North",
    [2] = "South",
    [3] = "East",
    [4] = "West",
})
```

See [Section 21](#21-runtime-library) for the definition of `__luao_enum_freeze`.

---

## 13. Property Getters and Setters

Property getters and setters allow computed or validated access to fields using standard dot syntax.

### Syntax

```
class Temperature
    private _celsius: number

    new(celsius: number)
        self._celsius = celsius
    end

    public get celsius(): number
        return self._celsius
    end

    public set celsius(value: number)
        if value < -273.15 then
            error("Temperature below absolute zero")
        end
        self._celsius = value
    end

    public get fahrenheit(): number
        return self._celsius * 9 / 5 + 32
    end

    public set fahrenheit(value: number)
        self.celsius = (value - 32) * 5 / 9
    end
end
```

### Usage

```
local t = Temperature.new(100)
print(t.celsius)     -- 100 (calls getter)
print(t.fahrenheit)  -- 212 (calls getter)
t.fahrenheit = 32    -- calls setter
print(t.celsius)     -- 0
```

### Semantics

- A getter is invoked when reading `obj.property`.
- A setter is invoked when writing `obj.property = value`.
- A property with only a getter is effectively readonly at runtime.
- A property with only a setter is write-only (uncommon but permitted).
- Getters and setters may have access modifiers.

### Transpilation

Getters and setters transpile to `__index` and `__newindex` interceptor functions on the class metatable. The transpiler generates a dispatch table for properties with getters/setters.

```lua
-- LUA output
local Temperature = {}

local Temperature_getters = {
    celsius = function(self)
        return self._celsius
    end,
    fahrenheit = function(self)
        return self._celsius * 9 / 5 + 32
    end,
}

local Temperature_setters = {
    celsius = function(self, value)
        if value < -273.15 then
            error("Temperature below absolute zero")
        end
        self._celsius = value
    end,
    fahrenheit = function(self, value)
        Temperature_setters.celsius(self, (value - 32) * 5 / 9)
    end,
}

Temperature.__index = function(self, key)
    local getter = Temperature_getters[key]
    if getter then return getter(self) end
    return Temperature[key]
end

Temperature.__newindex = function(self, key, value)
    local setter = Temperature_setters[key]
    if setter then setter(self, value); return end
    rawset(self, key, value)
end

function Temperature.new(celsius)
    local self = setmetatable({}, Temperature)
    self._celsius = celsius  -- uses rawset internally during construction
    return self
end
```

Note: When getters/setters are present, `__index` changes from a simple table reference to a function. The transpiler handles this automatically. Methods and inherited methods are still resolved through the function-based `__index`.

---

## 14. Type Annotations

Luao supports optional type annotations that are checked at compile time and erased during transpilation. They produce no runtime code.

### Syntax

Type annotations use a colon after the name:

```
local name: string = "Alice"
local age: number = 30
local active: boolean = true
```

### Built-in Types

| Type       | Description                                    |
|------------|------------------------------------------------|
| `string`   | Lua string                                     |
| `number`   | Lua number (integer or float)                  |
| `boolean`  | `true` or `false`                              |
| `nil`      | The nil value                                  |
| `table`    | Any Lua table                                  |
| `function` | Any Lua function                               |
| `any`      | Disables type checking for this value          |
| `void`     | No return value (only valid as return type)     |

### Union Types

A value may have one of several types:

```
local id: string | number = "abc"
id = 42  -- OK
```

### Optional Types

The `?` suffix is syntactic sugar for a union with `nil`:

```
local name: string? = nil  -- equivalent to: string | nil
```

### Array Types

The `[]` suffix denotes an array (a table with consecutive integer keys):

```
local scores: number[] = {95, 87, 100}
```

### Table Types

Generic table types specify key and value types:

```
local ages: Table<string, number> = { Alice = 30, Bob = 25 }
```

### Function Types

Function types use arrow syntax:

```
local callback: (number, string) -> boolean
local transform: (number) -> number
local producer: () -> string
```

### Generic Type Parameters

Classes and functions may be parameterized by type variables:

```
class Stack<T>
    private items: T[]

    new()
        self.items = {}
    end

    public function push(item: T): void
        table.insert(self.items, item)
    end

    public function pop(): T?
        return table.remove(self.items)
    end

    public function peek(): T?
        return self.items[#self.items]
    end
end

local s = Stack.new<number>()  -- or inferred from usage
s:push(42)
```

Generic type parameters are erased at transpile time.

### Function Return Type Annotations

```
function add(a: number, b: number): number
    return a + b
end

function greet(name: string): string
    return "Hello, " .. name
end
```

### Variable Declarations

```
local count: number = 0
local name: string
local items: string[] = {}
```

### Type Annotations on Fields

See [Section 2](#2-classes) for field type annotations within classes.

### Type Erasure

All type annotations are removed during transpilation. They have zero runtime cost.

```
-- LUAO
local x: number = 42
function add(a: number, b: number): number
    return a + b
end

-- LUA output
local x = 42
function add(a, b)
    return a + b
end
```

---

## 15. The `override` Keyword

The `override` keyword explicitly marks a method as overriding a parent class method.

### Syntax

```
class Dog extends Animal
    override function speak(): string
        return "Woof"
    end
end
```

### Strict Mode Behavior

When the checker operates in **strict mode**, the `override` keyword is **required** on any method that overrides a parent method. Omitting it produces a compile error.

In non-strict mode, `override` is optional but still validated: if present, the checker verifies that a parent method with the same name exists.

### Rules

1. If `override` is present and no parent method with that name exists, it is a compile error.
2. In strict mode, if a method shadows a parent method without `override`, it is a compile error.
3. `override` is not valid on static methods, constructors, or methods in classes without a parent.

### Example

```
class Animal
    public function speak(): string
        return "..."
    end

    public function breathe(): void
        -- ...
    end
end

class Dog extends Animal
    override function speak(): string   -- OK: Animal has speak()
        return "Woof"
    end

    override function fly(): void       -- COMPILE ERROR: Animal has no fly()
        -- ...
    end

    function breathe(): void            -- COMPILE ERROR in strict mode:
        -- ...                          -- missing 'override' keyword
    end
end
```

### Transpilation

The `override` keyword is erased during transpilation. It has no runtime effect.

---

## 16. The `instanceof` Operator

The `instanceof` operator performs a runtime check to determine whether a value is an instance of a given class, considering the full inheritance chain.

### Syntax

```
if dog instanceof Animal then
    print("It's an animal!")
end
```

### Semantics

`value instanceof Class` evaluates to `true` if and only if `Class` appears anywhere in the metatable chain of `value`. This means:

- `dog instanceof Dog` is `true` if `dog` was created by `Dog.new`.
- `dog instanceof Animal` is `true` if `Dog extends Animal`.
- `dog instanceof Unrelated` is `false`.

### Transpilation

```lua
-- LUA output
if __luao_instanceof(dog, Animal) then
    print("It's an animal!")
end
```

See [Section 21](#21-runtime-library) for the definition of `__luao_instanceof`.

### Usage with Variables

`instanceof` is an infix operator with the same precedence as comparison operators:

```
local isAnimal: boolean = dog instanceof Animal
```

### Limitations

- The right-hand operand must be a class name (a table with a metatable chain). Using a non-class value is undefined behavior.
- `instanceof` does not work with interfaces, since interfaces produce no runtime representation. Use type annotations and compile-time checking instead.

---

## 17. Generators

Generator functions produce a sequence of values lazily using the `yield` keyword. They transpile to Lua coroutines and work directly with Lua's `for...in` loop.

### Syntax

A generator function is declared with the `generator` keyword before `function`:

```
generator function range(start: number, stop: number): number
    for i = start, stop do
        yield i
    end
end
```

### The `yield` Keyword

- `yield value` suspends the generator and produces `value` to the caller.
- `yield` with no value produces `nil`.
- `yield` is only valid inside a generator function. Using it elsewhere is a compile error (E021).

### Usage

Generator functions return an iterator function compatible with Lua's generic `for`:

```
for v in range(1, 5) do
    print(v) -- 1, 2, 3, 4, 5
end
```

### Generator Methods

Class methods may be generators:

```
class NumberRange
    private start: number
    private stop: number

    new(start: number, stop: number)
        self.start = start
        self.stop = stop
    end

    generator function values(): number
        for i = self.start, self.stop do
            yield i
        end
    end

    generator function evens(): number
        for i = self.start, self.stop do
            if i % 2 == 0 then
                yield i
            end
        end
    end
end

local r = NumberRange.new(1, 10)
for v in r:evens() do
    print(v) -- 2, 4, 6, 8, 10
end
```

### Anonymous Generators

Generator function expressions are also supported:

```
local squares = generator function(n: number): number
    for i = 1, n do
        yield i * i
    end
end

for s in squares(5) do
    print(s) -- 1, 4, 9, 16, 25
end
```

### Transpilation

Generator functions transpile to `coroutine.wrap`, which returns an iterator function. `yield` transpiles to `coroutine.yield`. There is zero runtime library overhead — only standard Lua coroutine functions are used.

```lua
-- LUA output
function range(start, stop)
    return coroutine.wrap(function()
        for i = start, stop do
            coroutine.yield(i)
        end
    end)
end
```

For class methods:

```lua
-- LUA output
function NumberRange:values()
    return coroutine.wrap(function()
        for i = self.start, self.stop do
            coroutine.yield(i)
        end
    end)
end
```

The outer method's `self` is captured by the inner closure automatically via Lua's upvalue mechanism.

---

## 18. Async and Await

Async functions enable coroutine-based concurrency. An async function returns a **Task** object that can be chained with callbacks or awaited from other async functions.

### Syntax

An async function is declared with the `async` keyword before `function`:

```
async function fetchData(url: string): string
    local response = await httpGet(url)
    return response
end
```

### The `await` Keyword

- `await expr` suspends the current async function until `expr` resolves.
- If `expr` is a Task (returned by another async function), the current function resumes when that Task completes.
- If `expr` is a plain value, the function resumes immediately with that value.
- `await` is only valid inside an async function. Using it elsewhere is a compile error (E022).

### Task Objects

Async functions return a Task object with the following interface:

| Method | Description |
|--------|-------------|
| `task:andThen(callback)` | Registers a callback `function(result, err)` called when the task completes. Returns the task for chaining. |
| `task:await()` | Blocks (via coroutine yields) until the task completes. Returns the result or raises the error. |
| `task._status` | `"pending"`, `"resolved"`, or `"rejected"` |
| `task._result` | The resolved value (when status is `"resolved"`) |
| `task._error` | The error value (when status is `"rejected"`) |

### Usage

```
-- Fire and chain
local task = fetchData("http://example.com")
task:andThen(function(result, err)
    if err then
        print("Error: " .. err)
    else
        print("Got: " .. result)
    end
end)

-- Await inside another async function
async function pipeline(): string
    local raw = await fetchData("http://example.com")
    local processed = await transform(raw)
    return processed
end
```

### Async Methods

Class methods may be async:

```
class Api
    private baseUrl: string

    new(baseUrl: string)
        self.baseUrl = baseUrl
    end

    async function get(path: string): table
        return await httpGet(self.baseUrl .. path)
    end

    async function getUser(id: number): table
        return await self:get("/users/" .. id)
    end
end
```

### Error Handling

If an async function throws an error (via `error()`), the task is rejected. The error is passed as the second argument to `andThen` callbacks:

```
async function failingTask(): void
    error("something went wrong")
end

failingTask():andThen(function(result, err)
    print(err) -- "something went wrong"
end)
```

### Nested Awaits

When an async function awaits another Task, the runtime automatically chains them. The outer task remains pending until all inner awaits resolve:

```
async function step1(): number
    return 10
end

async function step2(): number
    local x = await step1()
    return x * 2
end

async function step3(): number
    local y = await step2()
    return y + 1
end

step3():andThen(function(result, err)
    print(result) -- 21
end)
```

### Transpilation

Async functions transpile to `__luao_async(function() ... end)`. `await` transpiles to `__luao_await(expr)`.

```lua
-- LUA output
function fetchData(url)
    return __luao_async(function()
        local response = __luao_await(httpGet(url))
        return response
    end)
end
```

For class methods:

```lua
-- LUA output
function Api:get(path)
    return __luao_async(function()
        return __luao_await(httpGet(self.baseUrl .. path))
    end)
end
```

See [Section 21](#21-runtime-library) for the definitions of `__luao_async` and `__luao_await`.

---

## 19. Grammar (EBNF)

This section provides a formal EBNF grammar for all Luao-specific syntactic extensions, including generators and async/await. The base Lua 5.4 grammar is assumed as defined in the [Lua 5.4 Reference Manual, Section 9](https://www.lua.org/manual/5.4/manual.html#9) and is extended with the following productions.

Terminals are shown in `'single quotes'` or as `UPPER_CASE` token names. Non-terminals are in `CamelCase`. `{ X }` means zero or more repetitions of X. `[ X ]` means X is optional. `( X | Y )` means a choice between X and Y.

```ebnf
(* ============================================================ *)
(* Top-level: extends Lua 5.4 stat production                   *)
(* ============================================================ *)

stat        = lua54_stat
            | classdecl
            | interfacedecl
            | enumdecl
            | genfuncdecl
            | asyncfuncdecl ;

(* ============================================================ *)
(* Class Declaration                                             *)
(* ============================================================ *)

classdecl   = { classmod } , 'class' , NAME ,
              [ genericparams ] ,
              [ 'extends' , NAME [ genericargs ] ] ,
              [ 'implements' , namelist ] ,
              classbody ,
              'end' ;

classmod    = 'abstract'
            | 'sealed' ;

classbody   = { classmember } ;

classmember = fielddecl
            | constructordecl
            | methoddecl
            | getprop
            | setprop ;

(* ============================================================ *)
(* Field Declaration                                             *)
(* ============================================================ *)

fielddecl   = [ accessmod ] , [ 'static' ] , [ 'readonly' ] ,
              NAME , [ ':' , type ] , [ '=' , exp ] ;

(* ============================================================ *)
(* Constructor                                                   *)
(* ============================================================ *)

constructordecl = 'new' , '(' , [ parlist ] , ')' , block , 'end' ;

(* ============================================================ *)
(* Method Declaration                                            *)
(* ============================================================ *)

methoddecl  = [ accessmod ] , [ 'static' ] , [ 'abstract' ] ,
              [ 'override' ] , [ 'async' ] , [ 'generator' ] ,
              'function' , NAME ,
              [ genericparams ] ,
              '(' , [ parlist ] , ')' ,
              [ ':' , type ] ,
              ( block , 'end' | (* empty for abstract *) ) ;

(* Note: abstract methods have no body and no trailing 'end'. *)

(* ============================================================ *)
(* Getters and Setters                                           *)
(* ============================================================ *)

getprop     = [ accessmod ] , 'get' , NAME ,
              '(' , ')' , [ ':' , type ] ,
              block , 'end' ;

setprop     = [ accessmod ] , 'set' , NAME ,
              '(' , NAME , [ ':' , type ] , ')' ,
              block , 'end' ;

(* ============================================================ *)
(* Access Modifiers                                              *)
(* ============================================================ *)

accessmod   = 'public'
            | 'private'
            | 'protected' ;

(* ============================================================ *)
(* Interface Declaration                                         *)
(* ============================================================ *)

interfacedecl = 'interface' , NAME ,
                [ genericparams ] ,
                [ 'extends' , namelist ] ,
                interfacebody ,
                'end' ;

interfacebody = { interfacemember } ;

interfacemember = interfacemethod
                | interfacefield ;

interfacemethod = 'function' , NAME ,
                  [ genericparams ] ,
                  '(' , [ parlist ] , ')' ,
                  [ ':' , type ] ;

interfacefield  = NAME , ':' , type ;

(* ============================================================ *)
(* Enum Declaration                                              *)
(* ============================================================ *)

enumdecl    = 'enum' , NAME ,
              enumbody ,
              'end' ;

enumbody    = { enumentry } ;

enumentry   = NAME , [ '=' , ( NUMERAL | LITERALSTRING ) ] ;

(* ============================================================ *)
(* Type Annotations                                              *)
(* ============================================================ *)

type        = simpletype , { '|' , simpletype } ;

simpletype  = 'string' | 'number' | 'boolean' | 'nil'
            | 'table' | 'function' | 'any' | 'void'
            | NAME [ genericargs ]
            | simpletype , '[]'
            | simpletype , '?'
            | 'Table' , '<' , type , ',' , type , '>'
            | '(' , [ typelist ] , ')' , '->' , type
            | '(' , type , ')' ;

typelist    = type , { ',' , type } ;

genericparams = '<' , NAME , { ',' , NAME } , '>' ;

genericargs   = '<' , type , { ',' , type } , '>' ;

(* ============================================================ *)
(* Parameter List (extends Lua 5.4 parlist)                      *)
(* ============================================================ *)

parlist     = param , { ',' , param } , [ ',' , '...' [ ':' , type ] ]
            | '...' [ ':' , type ] ;

param       = NAME , [ ':' , type ] , [ '=' , exp ] ;

(* ============================================================ *)
(* Name List                                                     *)
(* ============================================================ *)

namelist    = NAME , { ',' , NAME } ;

(* ============================================================ *)
(* Generator Function Declaration                                *)
(* ============================================================ *)

genfuncdecl = [ 'local' ] , 'generator' , 'function' , NAME ,
              [ genericparams ] ,
              '(' , [ parlist ] , ')' ,
              [ ':' , type ] ,
              block , 'end' ;

(* ============================================================ *)
(* Async Function Declaration                                    *)
(* ============================================================ *)

asyncfuncdecl = [ 'local' ] , 'async' , [ 'generator' ] ,
                'function' , NAME ,
                [ genericparams ] ,
                '(' , [ parlist ] , ')' ,
                [ ':' , type ] ,
                block , 'end' ;

(* ============================================================ *)
(* Expression Extensions                                         *)
(* ============================================================ *)

exp         = lua54_exp
            | exp , 'instanceof' , NAME
            | 'super' , '.' , NAME , '(' , [ explist ] , ')'
            | 'yield' , [ exp ]
            | 'await' , exp ;

(* ============================================================ *)
(* Super Expressions                                             *)
(* ============================================================ *)

(* super.method(args)  — parent method call *)
(* super.new(args)     — parent constructor call *)
(* These are parsed as special call expressions; 'super' is     *)
(* not a general-purpose expression.                             *)
```

### Notes on the Grammar

1. **Ambiguity resolution.** The `abstract` keyword may appear as both a class modifier (`abstract class`) and a method modifier (`abstract function`). The parser resolves this by context: before `class` it is a class modifier; before `function` within a class body it is a method modifier.

2. **Abstract method termination.** Abstract methods do not have a body or a trailing `end`. The parser recognizes an abstract method by the presence of the `abstract` keyword before `function` and expects the declaration to end after the optional return type annotation.

3. **Operator precedence.** The `instanceof` operator has the same precedence level as comparison operators (`<`, `>`, `<=`, `>=`, `==`, `~=`).

4. **Contextual keywords.** The following identifiers are contextual keywords, meaning they are only reserved in specific syntactic positions and may still be used as variable names in Lua code: `class`, `interface`, `enum`, `extends`, `implements`, `abstract`, `sealed`, `static`, `readonly`, `override`, `public`, `private`, `protected`, `get`, `set`, `new`, `super`, `instanceof`, `async`, `await`, `yield`, `generator`.

5. **Yield expression termination.** `yield` with no following expression produces `nil`. The parser determines whether a value follows by checking if the next token can begin an expression.

6. **Await precedence.** `await` has the same precedence as unary operators (`not`, `-`, `#`, `~`).

---

## 20. Transpilation Reference

This section shows complete input/output pairs for each major feature.

### 20.1 Basic Class

**LUAO input:**

```
class Point
    public x: number
    public y: number

    new(x: number, y: number)
        self.x = x
        self.y = y
    end

    public function distanceTo(other: Point): number
        local dx = self.x - other.x
        local dy = self.y - other.y
        return math.sqrt(dx * dx + dy * dy)
    end

    function __tostring(): string
        return "(" .. self.x .. ", " .. self.y .. ")"
    end
end
```

**Lua output:**

```lua
local Point = {}
Point.__index = Point

function Point.new(x, y)
    local self = setmetatable({}, Point)
    self.x = x
    self.y = y
    return self
end

function Point:distanceTo(other)
    local dx = self.x - other.x
    local dy = self.y - other.y
    return math.sqrt(dx * dx + dy * dy)
end

function Point:__tostring()
    return "(" .. self.x .. ", " .. self.y .. ")"
end
```

### 20.2 Inheritance and `super`

**LUAO input:**

```
class Animal
    public name: string

    new(name: string)
        self.name = name
    end

    public function speak(): string
        return self.name .. " makes a sound"
    end
end

class Dog extends Animal
    public breed: string

    new(name: string, breed: string)
        super.new(name)
        self.breed = breed
    end

    override function speak(): string
        return self.name .. " barks"
    end
end
```

**Lua output:**

```lua
local Animal = {}
Animal.__index = Animal

function Animal.new(name)
    local self = setmetatable({}, Animal)
    self.name = name
    return self
end

function Animal:speak()
    return self.name .. " makes a sound"
end

local Dog = {}
Dog.__index = Dog
setmetatable(Dog, { __index = Animal })

function Dog.new(name, breed)
    local self = setmetatable({}, Dog)
    Animal.new(self, name)
    self.breed = breed
    return self
end

function Dog:speak()
    return self.name .. " barks"
end
```

### 20.3 Abstract Class

**LUAO input:**

```
abstract class Shape
    public color: string

    new(color: string)
        self.color = color
    end

    abstract function area(): number
end

class Circle extends Shape
    public radius: number

    new(color: string, radius: number)
        super.new(color)
        self.radius = radius
    end

    override function area(): number
        return math.pi * self.radius ^ 2
    end
end
```

**Lua output:**

```lua
local Shape = {}
Shape.__index = Shape

function Shape.new(color)
    local self = setmetatable({}, Shape)
    __luao_abstract_guard(self, Shape, "Shape")
    self.color = color
    return self
end

local Circle = {}
Circle.__index = Circle
setmetatable(Circle, { __index = Shape })

function Circle.new(color, radius)
    local self = setmetatable({}, Circle)
    Shape.new(self, color)
    self.radius = radius
    return self
end

function Circle:area()
    return math.pi * self.radius ^ 2
end
```

### 20.4 Static Members

**LUAO input:**

```
class Counter
    static count: number = 0

    new()
        Counter.count = Counter.count + 1
    end

    static function getCount(): number
        return Counter.count
    end

    static function reset(): void
        Counter.count = 0
    end
end
```

**Lua output:**

```lua
local Counter = {}
Counter.__index = Counter
Counter.count = 0

function Counter.new()
    local self = setmetatable({}, Counter)
    Counter.count = Counter.count + 1
    return self
end

function Counter.getCount()
    return Counter.count
end

function Counter.reset()
    Counter.count = 0
end
```

### 20.5 Interfaces

**LUAO input:**

```
interface Serializable
    function serialize(): string
    function deserialize(data: string): void
end

class User implements Serializable
    public name: string

    new(name: string)
        self.name = name
    end

    public function serialize(): string
        return self.name
    end

    public function deserialize(data: string): void
        self.name = data
    end
end
```

**Lua output:**

```lua
-- No code emitted for Serializable

local User = {}
User.__index = User

function User.new(name)
    local self = setmetatable({}, User)
    self.name = name
    return self
end

function User:serialize()
    return self.name
end

function User:deserialize(data)
    self.name = data
end
```

### 20.6 Enum

**LUAO input:**

```
enum Color
    Red
    Green
    Blue
end
```

**Lua output:**

```lua
local Color = __luao_enum_freeze("Color", {
    Red = 1,
    Green = 2,
    Blue = 3,
}, {
    [1] = "Red",
    [2] = "Green",
    [3] = "Blue",
})
```

### 20.7 Property Getters/Setters

**LUAO input:**

```
class Person
    private _name: string

    new(name: string)
        self._name = name
    end

    public get name(): string
        return self._name
    end

    public set name(value: string)
        if #value == 0 then
            error("Name cannot be empty")
        end
        self._name = value
    end
end
```

**Lua output:**

```lua
local Person = {}

local Person_getters = {
    name = function(self)
        return self._name
    end,
}

local Person_setters = {
    name = function(self, value)
        if #value == 0 then
            error("Name cannot be empty")
        end
        self._name = value
    end,
}

Person.__index = function(self, key)
    local getter = Person_getters[key]
    if getter then return getter(self) end
    return Person[key]
end

Person.__newindex = function(self, key, value)
    local setter = Person_setters[key]
    if setter then setter(self, value); return end
    rawset(self, key, value)
end

function Person.new(name)
    local self = setmetatable({}, Person)
    rawset(self, "_name", name)
    return self
end
```

Note: Inside the constructor, `rawset` is used for initial field assignment when `__newindex` interceptors are present, to avoid triggering setters during construction of backing fields.

### 20.8 `instanceof`

**LUAO input:**

```
local d = Dog.new("Rex", "Labrador")
if d instanceof Animal then
    print("Is an animal")
end
```

**Lua output:**

```lua
local d = Dog.new("Rex", "Labrador")
if __luao_instanceof(d, Animal) then
    print("Is an animal")
end
```

### 20.9 Type Annotations (Erasure)

**LUAO input:**

```
local items: string[] = {"a", "b", "c"}
local lookup: Table<string, number> = { a = 1 }
local callback: (number) -> string = function(n) return tostring(n) end

function process(data: string, count: number): boolean
    return #data > count
end
```

**Lua output:**

```lua
local items = {"a", "b", "c"}
local lookup = { a = 1 }
local callback = function(n) return tostring(n) end

function process(data, count)
    return #data > count
end
```

### 20.10 Generics (Erasure)

**LUAO input:**

```
class Pair<A, B>
    public first: A
    public second: B

    new(first: A, second: B)
        self.first = first
        self.second = second
    end
end

local p = Pair.new<string, number>("hello", 42)
```

**Lua output:**

```lua
local Pair = {}
Pair.__index = Pair

function Pair.new(first, second)
    local self = setmetatable({}, Pair)
    self.first = first
    self.second = second
    return self
end

local p = Pair.new("hello", 42)
```

### 20.11 Generator Function

**LUAO input:**

```
generator function range(start, stop)
    for i = start, stop do
        yield i
    end
end
```

**Lua output:**

```lua
function range(start, stop)
    return coroutine.wrap(function()
        for i = start, stop do
            coroutine.yield(i)
        end
    end)
end
```

### 20.12 Generator Class Method

**LUAO input:**

```
class Range
    private start: number
    private stop: number

    new(start: number, stop: number)
        self.start = start
        self.stop = stop
    end

    generator function values(): number
        for i = self.start, self.stop do
            yield i
        end
    end
end
```

**Lua output:**

```lua
local Range = {}
Range.__index = Range

function Range.new(start, stop)
    local self = setmetatable({}, Range)
    Range._init(self, start, stop)
    return self
end

function Range._init(self, start, stop)
    self.start = start
    self.stop = stop
end

function Range:values()
    return coroutine.wrap(function()
        for i = self.start, self.stop do
            coroutine.yield(i)
        end
    end)
end
```

### 20.13 Async Function

**LUAO input:**

```
async function fetchData(url: string): string
    local response = await httpGet(url)
    return response
end
```

**Lua output:**

```lua
function fetchData(url)
    return __luao_async(function()
        local response = __luao_await(httpGet(url))
        return response
    end)
end
```

### 20.14 Async Class Method

**LUAO input:**

```
class Api
    private baseUrl: string

    new(baseUrl: string)
        self.baseUrl = baseUrl
    end

    async function get(path: string): table
        return await httpGet(self.baseUrl .. path)
    end
end
```

**Lua output:**

```lua
local Api = {}
Api.__index = Api

function Api.new(baseUrl)
    local self = setmetatable({}, Api)
    Api._init(self, baseUrl)
    return self
end

function Api._init(self, baseUrl)
    self.baseUrl = baseUrl
end

function Api:get(path)
    return __luao_async(function()
        return __luao_await(httpGet(self.baseUrl .. path))
    end)
end
```

---

## 21. Runtime Library

Luao requires a small runtime library that is emitted (or required) at the top of transpiled files when the corresponding features are used. The runtime functions are prefixed with `__luao_` to avoid collisions.

### `__luao_instanceof(obj, class)`

Tests whether `obj` is an instance of `class` by walking the metatable chain.

```lua
function __luao_instanceof(obj, class)
    if type(obj) ~= "table" then
        return false
    end
    local mt = getmetatable(obj)
    while mt do
        if mt == class then
            return true
        end
        -- Walk up the inheritance chain
        local parent = getmetatable(mt)
        if parent then
            mt = parent.__index
            -- __index could be a table (the parent class) or a function
            if type(mt) == "function" then
                return false
            end
        else
            mt = nil
        end
    end
    return false
end
```

**Behavior:**

- Returns `false` if `obj` is not a table.
- Gets the metatable of `obj` (which is the class table for Luao instances).
- Compares the metatable to `class`. If they match, returns `true`.
- Otherwise, follows the metatable chain upward: gets the metatable of the current class table (set by `setmetatable(Child, { __index = Parent })`), extracts `__index`, and repeats.
- Returns `false` if the chain is exhausted without finding `class`.

### `__luao_abstract_guard(self, class, className)`

Prevents direct instantiation of abstract classes.

```lua
function __luao_abstract_guard(self, class, className)
    if getmetatable(self) == class then
        error("Cannot instantiate abstract class '" .. className .. "'", 2)
    end
end
```

**Behavior:**

- Called at the beginning of an abstract class constructor.
- If `self`'s metatable is the abstract class itself (meaning we are not constructing a subclass), raises an error.
- When a subclass constructor calls `super.new(...)`, `self`'s metatable is the subclass, so the guard passes.

### `__luao_enum_freeze(name, entries, reverseMap)`

Creates an immutable enum table with reverse lookup.

```lua
function __luao_enum_freeze(name, entries, reverseMap)
    entries._values = reverseMap
    return setmetatable({}, {
        __index = entries,
        __newindex = function(_, key, _)
            error("Attempt to modify frozen enum '" .. name .. "' (key: " .. tostring(key) .. ")", 2)
        end,
        __pairs = function(_)
            return next, entries, nil
        end,
        __tostring = function(_)
            return "enum " .. name
        end,
    })
end
```

**Behavior:**

- Stores the forward mapping (`Name -> value`) and reverse mapping (`value -> Name`) in an inner table.
- Returns a proxy table with a metatable that:
  - Allows reading entries via `__index`.
  - Prevents modification via `__newindex` (raises an error).
  - Supports `pairs()` iteration via `__pairs`.
  - Returns a descriptive string via `__tostring`.

### `__luao_async(fn)`

Creates a Task from a function. The function's body may call `__luao_await` to suspend until an awaited value resolves.

```lua
function __luao_async(fn)
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
```

**Behavior:**

- Creates a coroutine from `fn` and wraps it in a Task object.
- Auto-starts execution immediately. If `fn` completes synchronously (no awaits), the task resolves before `__luao_async` returns.
- When the coroutine yields (via `__luao_await`), the yielded value is inspected:
  - If it is a Task (has a `_status` field), the current task chains onto it via `andThen`.
  - If it is a plain value, the coroutine is resumed immediately with that value.
- On completion, all registered `andThen` callbacks are invoked with `(result, nil)` for success or `(nil, error)` for failure.
- `task:await()` is a blocking wait using coroutine yields, suitable for use inside other coroutines.

### `__luao_await(value)`

Yields the current coroutine, passing `value` to the Task scheduler.

```lua
function __luao_await(value)
    return coroutine.yield(value)
end
```

**Behavior:**

- Suspends the current async function's coroutine.
- The Task scheduler (in `__luao_async`) inspects the yielded value and determines how to resume.
- When the awaited value resolves, the coroutine is resumed with the result.

### Runtime Inclusion Strategy

The runtime functions are included in transpiled output using one of two strategies, selected by the transpiler configuration:

1. **Inline mode (default).** The runtime functions used by the file are emitted directly at the top of the output file. Only functions actually needed are included.

2. **Module mode.** A `require("luao_runtime")` statement is emitted, and the runtime functions are provided as a separate Lua module. This is preferred for multi-file projects to avoid duplication.

---

## Appendix A: Reserved Words

Luao adds the following contextual keywords to Lua 5.4. These words are reserved only in syntactic positions where Luao extensions are expected. They remain usable as identifiers in plain Lua code.

| Keyword       | Context                              |
|---------------|--------------------------------------|
| `abstract`    | Class and method modifier            |
| `class`       | Class declaration                    |
| `enum`        | Enum declaration                     |
| `extends`     | Class/interface inheritance          |
| `get`         | Property getter                      |
| `implements`  | Interface implementation             |
| `instanceof`  | Type-checking operator               |
| `interface`   | Interface declaration                |
| `new`         | Constructor declaration              |
| `override`    | Method override marker               |
| `private`     | Access modifier                      |
| `protected`   | Access modifier                      |
| `public`      | Access modifier                      |
| `readonly`    | Field modifier                       |
| `sealed`      | Class modifier                       |
| `set`         | Property setter                      |
| `static`      | Static member modifier               |
| `super`       | Parent class reference               |
| `async`       | Async function modifier              |
| `await`       | Async suspension expression          |
| `yield`       | Generator value production           |
| `generator`   | Generator function modifier          |

## Appendix B: Compilation Errors

The following is a non-exhaustive list of compile-time errors specific to Luao features.

| Code   | Description                                                              |
|--------|--------------------------------------------------------------------------|
| E001   | Cannot instantiate abstract class                                        |
| E002   | Non-abstract class does not implement abstract method `X`                |
| E003   | Class with abstract methods must be declared `abstract`                  |
| E004   | Cannot extend sealed class `X` from a different file                     |
| E005   | Multiple inheritance is not supported                                    |
| E006   | `super` used outside of a class with a parent                            |
| E007   | `override` specified but no parent method `X` exists                     |
| E008   | Method `X` overrides parent method without `override` keyword (strict)   |
| E009   | Cannot access private member `X` of class `Y`                            |
| E010   | Cannot access protected member `X` of class `Y` from unrelated class     |
| E011   | Assignment to readonly field `X` outside of constructor                  |
| E012   | Class `X` does not implement interface method `Y`                        |
| E013   | Duplicate enum entry `X`                                                 |
| E014   | Mixed auto-increment and string values in enum                           |
| E015   | Static method cannot reference `self`                                    |
| E016   | Constructor must not return a value                                      |
| E017   | Duplicate class member `X`                                               |
| E018   | Type mismatch: expected `X`, found `Y`                                   |
| E019   | `instanceof` right-hand side must be a class name                        |
| E020   | Cannot declare more than one constructor per class                       |
| E021   | `yield` used outside of a generator function                             |
| E022   | `await` used outside of an async function                                |

## Appendix C: Compatibility Notes

### Lua 5.4 Features

All Lua 5.4 features are fully supported, including:

- Integer/float number subtypes
- Bitwise operators (`&`, `|`, `~`, `<<`, `>>`)
- Integer division (`//`)
- `goto` and labels
- Generational garbage collection
- `<const>` and `<close>` local variable attributes
- All standard libraries (`string`, `table`, `math`, `io`, `os`, `coroutine`, `debug`, `utf8`, `package`)

### Interaction with Metatables

Luao's class system uses metatables internally. If user code manually calls `setmetatable` on a class instance, the class's method dispatch and `instanceof` behavior may break. Users should avoid manually modifying metatables on Luao class instances.

### Interop with Plain Lua

Luao transpiled code is standard Lua 5.4 and can interop freely with existing Lua code:

- Luao classes can be passed to and from plain Lua functions.
- Plain Lua tables can be passed to Luao code but will not satisfy `instanceof` checks.
- Luao modules can be `require`d from plain Lua and vice versa.
