# Luao

A superset of Lua with classes, interfaces, enums, and more — all transpiling down to plain Lua tables and metatables.

```lua
class Animal
    private name: string

    new(name: string)
        self.name = name
    end

    public function speak(): string
        return "..."
    end
end

class Dog extends Animal
    new(name: string)
        super.new(name)
    end

    override function speak(): string
        return "Woof!"
    end
end

local dog = new Dog("Rex")
print(dog:speak())
```

Transpiles to:

```lua
local Animal = {}
Animal.__index = Animal

function Animal._new(name)
    local self = setmetatable({}, Animal)
    self._name = name
    return self
end

function Animal:speak()
    return "..."
end

local Dog = setmetatable({}, { __index = Animal })
Dog.__index = Dog

function Dog._new(name)
    local self = Animal._new(name)
    setmetatable(self, Dog)
    return self
end

function Dog:speak()
    return "Woof!"
end

local dog = Dog._new("Rex")
print(dog:speak())
```

## Install

```bash
git clone https://github.com/callisto-corp/luao.git
cd luao
cargo build --release
```

The binary lands at `target/release/luao` (or `luao.exe` on Windows).

Add it to your PATH:

```bash
# Linux/macOS
export PATH="$PATH:$(pwd)/target/release"

# Windows (PowerShell)
$env:Path += ";$(Get-Location)\target\release"

# Windows (permanent)
[System.Environment]::SetEnvironmentVariable('Path', $env:Path + ';D:\path\to\luao\target\release', 'User')
```

## Usage

### Transpile

```bash
# Single file
luao build myfile.luao

# Entire directory (recursive)
luao build src/
```

This outputs `.lua` files next to each `.luao` source.

### Check

```bash
luao check myfile.luao
```

Runs the parser, resolver, and semantic checker without writing output. Prints any errors or "OK".

### LSP

```bash
luao lsp
```

Starts the language server on stdin/stdout. You don't run this manually — VSCode does it for you.

## VSCode Extension

```bash
cd vscode-luao
npm install
npm run compile
npx @vscode/vsce package --allow-missing-repository
code --install-extension luao-0.1.0.vsix
```

Then set the binary path in VSCode settings (`Ctrl+,`):

```json
{
  "luao.binaryPath": "/path/to/luao"
}
```

Open any `.luao` file and you get:

- Syntax highlighting for all Luao keywords
- Real-time error diagnostics
- Autocompletion for keywords and class members
- Hover information
- Go-to-definition
- Document symbols outline
- Semantic token highlighting

## Language Features

### Classes

```lua
class Player
    public name: string
    private health: number
    readonly id: number

    new(name: string, id: number)
        self.name = name
        self.health = 100
        self.id = id
    end

    public function take_damage(amount: number): void
        self.health = self.health - amount
        if self.health < 0 then
            self.health = 0
        end
    end

    public function is_alive(): boolean
        return self.health > 0
    end
end
```

### Inheritance

```lua
class Animal
    protected name: string

    new(name: string)
        self.name = name
    end

    public function speak(): string
        return "..."
    end
end

class Cat extends Animal
    new(name: string)
        super.new(name)
    end

    override function speak(): string
        return self.name .. " says meow"
    end
end
```

### Interfaces

Compile-time contracts — no runtime cost.

```lua
interface Drawable
    function draw(x: number, y: number): void
end

interface Resizable
    function resize(factor: number): void
end

class Sprite implements Drawable, Resizable
    private x: number
    private y: number
    private scale: number

    new()
        self.x = 0
        self.y = 0
        self.scale = 1
    end

    public function draw(x: number, y: number): void
        self.x = x
        self.y = y
    end

    public function resize(factor: number): void
        self.scale = self.scale * factor
    end
end
```

### Abstract Classes

```lua
abstract class Shape
    abstract function area(): number
    abstract function perimeter(): number

    public function describe(): string
        return "Shape with area " .. self:area()
    end
end

class Circle extends Shape
    private radius: number

    new(radius: number)
        self.radius = radius
    end

    override function area(): number
        return 3.14159 * self.radius * self.radius
    end

    override function perimeter(): number
        return 2 * 3.14159 * self.radius
    end
end
```

### Enums

Immutable at runtime. Auto-increment from 1 if no value given.

```lua
enum Direction
    North = 1
    South = 2
    East = 3
    West = 4
end

enum Color
    Red
    Green
    Blue
end

local dir = Direction.North
print(Direction._values[dir])  -- "North"
```

### Operator Overloading

Define metamethods as class methods.

```lua
class Vec2
    public x: number
    public y: number

    new(x: number, y: number)
        self.x = x
        self.y = y
    end

    public function __add(other: Vec2): Vec2
        return new Vec2(self.x + other.x, self.y + other.y)
    end

    public function __tostring(): string
        return "(" .. self.x .. ", " .. self.y .. ")"
    end
end

local a = new Vec2(1, 2)
local b = new Vec2(3, 4)
print(tostring(a + b))  -- (4, 6)
```

### Static Members

```lua
class MathUtils
    static PI: number = 3.14159

    static function lerp(a: number, b: number, t: number): number
        return a + (b - a) * t
    end
end

print(MathUtils.PI)
print(MathUtils.lerp(0, 100, 0.5))
```

### Sealed Classes

Cannot be extended outside the file they're defined in.

```lua
sealed class Config
    readonly host: string
    readonly port: number

    new(host: string, port: number)
        self.host = host
        self.port = port
    end
end
```

### Property Getters/Setters

```lua
class Temperature
    private _celsius: number

    new(celsius: number)
        self._celsius = celsius
    end

    public get celsius: number
        return self._celsius
    end

    public set celsius(value: number)
        if value < -273.15 then
            error("Below absolute zero")
        end
        self._celsius = value
    end
end

local t = new Temperature(100)
print(t.celsius)      -- 100 (calls getter)
t.celsius = 200       -- calls setter with validation
```

### instanceof

Runtime check that walks the metatable chain.

```lua
local dog = new Dog("Rex")
print(dog instanceof Animal)  -- true
print(dog instanceof Dog)     -- true
print(dog instanceof Cat)     -- false
```

### Type Annotations

Optional. Used by the checker and LSP. Erased at transpile time.

```lua
local name: string = "hello"
local items: number[] = {1, 2, 3}
local callback: (number, string) -> boolean
local maybe: string? = nil

class Stack<T>
    public function push(item: T): void
    public function pop(): T
end
```

### Access Modifiers

| Modifier | Access |
|---|---|
| `public` | Anywhere (default for methods) |
| `private` | Only within the declaring class |
| `protected` | Declaring class and subclasses |

Enforced at compile time. Private fields get a `_` prefix in the output as a safety net.

## Architecture

```
.luao source
    → luao-lexer (tokenize)
    → luao-parser (recursive descent → AST)
    → luao-resolver (name resolution → symbol table)
    → luao-checker (semantic analysis → diagnostics)
    → luao-transpiler (AST → Lua source)
    → full_moon (format output)
    → .lua output
```

7 Rust crates in a Cargo workspace:

| Crate | Purpose |
|---|---|
| `luao-lexer` | Tokenizer for Lua + Luao keywords |
| `luao-parser` | Recursive descent parser with Pratt expression parsing |
| `luao-resolver` | Scope tracking, symbol table construction |
| `luao-checker` | Access modifiers, abstract/sealed/interface/readonly enforcement |
| `luao-transpiler` | Code generation using `full_moon` for output formatting |
| `luao-lsp` | Language server with `tower-lsp` |
| `luao-cli` | CLI: `build`, `check`, `lsp` subcommands |

## Full Specification

See [SPECIFICATION.md](SPECIFICATION.md) for the complete language spec including formal EBNF grammar, transpilation reference for every feature, and runtime library documentation.

## License

MIT
