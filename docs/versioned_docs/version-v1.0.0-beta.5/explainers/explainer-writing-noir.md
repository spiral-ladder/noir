---
title: Thinking in Circuits
description: Considerations when writing Noir programs
keywords: [Noir, programming, rust]
tags: [Optimization]
sidebar_position: 0
---


This article intends to set you up with key concepts essential for writing more viable applications that use zero knowledge proofs, namely around efficient circuits.

## Context - 'Efficient' is subjective

When writing a web application for a performant computer with high-speed internet connection, writing efficient code sometimes is seen as an afterthought only if needed. Large multiplications running at the innermost of nested loops may not even be on a dev's radar.
When writing firmware for a battery-powered microcontroller, you think of cpu cycles as rations to keep within a product's power budget.

> Code is written to create applications that perform specific tasks within specific constraints

And these constraints differ depending on where the compiled code is execute.

### The Ethereum Virtual Machine (EVM)

In scenarios where extremely low gas costs are required for an Ethereum application to be viable/competitive, Ethereum smart contract developers get into what is colloquially known as: "*gas golfing*". Finding the lowest execution cost of their compiled code (EVM bytecode) to achieve a specific task.

The equivalent optimization task when writing zk circuits is affectionately referred to as "*gate golfing*", finding the lowest gate representation of the compiled Noir code.

### Coding for circuits - a paradigm shift

In zero knowledge cryptography, code is compiled to "circuits" consisting of arithmetic gates, and gate count is the significant cost. Depending on the proving system this is linearly proportionate to proof size and proving time, so from a product point of view this should be kept as low as possible.

Whilst writing efficient code for web apps and Solidity have some differences, writing efficient circuits have a different set of considerations. It is a bit of a paradigm shift, like writing code for GPUs for the first time...

For example, drawing a circle at (0, 0) of radius `r`:
- For a single CPU thread,
```
for theta in 0..2*pi {
  let x = r * cos(theta);
  let y = r * sin(theta);
  draw(x, y);
} // note: would do 0 - pi/2 and draw +ve/-ve x and y.
```

- For GPUs (simultaneous parallel calls with x, y across image),
```
if (x^2 + y^2 = r^2) {
  draw(x, y);
}
```

([Related](https://www.youtube.com/watch?v=-P28LKWTzrI))

Whilst this CPU -> GPU does not translate to circuits exactly, it is intended to exemplify the difference in intuition when coding for different machine capabilities/constraints.

### Context Takeaway

For those coming from a primarily web app background, this article will explain what you need to consider when writing circuits. Furthermore, for those experienced writing efficient machine code, prepare to shift what you think is efficient 😬

## Translating from Rust

Programs written in anything from pseudo code to C, can be translated into Noir. A Rust program written for execution can be readily ported to Noir thanks to the similarities in syntax.

:::note
Many valuable functions and algorithms have been written in more established languages (C/C++), and converted to modern ones (like Rust).
:::

Fortunately for Noir developers, when needing a particular function a Rust implementation can be readily compiled into Noir with some key changes. While the compiler does a decent amount of optimizations, it won't be able to change code that has been optimized for clock-cycles into code optimized for arithmetic gates.

A few things to do when converting Rust code to Noir:
- `println!` is not a macro, use `println` function (same for `assert_eq`)
- No early `return` in function. Use constrain via assertion instead
- No passing by reference. Remove `&` operator to pass by value (copy)
- No boolean operators (`&&`, `||`). Use bitwise operators (`&`, `|`) with boolean values
- No type `usize`. Use types `u8`, `u32`, `u64`, ...
- `main` return must be public, `pub`
- No `const`, use `global`
- Noir's LSP is your friend, so error message should be informative enough to resolve syntax issues.

## Writing efficient Noir for performant products

The following points help refine our understanding over time.

:::note
A Noir program makes a statement that can be verified.
:::

It compiles to a structure that represents the calculation, and can assert results within the calculation at any stage (via the `constrain` keyword).

A Noir program compiles to an Abstract Circuit Intermediate Representation which is:
 - Conceptually a tree structure
 - Leaves (inputs) are the `Field` type
 - Nodes contain arithmetic operations to combine them (gates)
 - The root is the final result (return value)

:::tip
The command `nargo info` shows the programs circuit size, and is useful to compare the value of changes made.
You can dig deeper and use the `--print-acir` param to take a closer look at individual ACIR opcodes, and the proving backend to see its gate count (eg for barretenberg, the `bb` binary has a `gates` option).
:::

### Numerical types

As mentioned earlier Noir has many familiar integer types (eg `i8`, `u64`). Ideally after bringing a program into Noir, proving/verifying of its execution just works where needed: client/server side, on an evm, or on the Aztec network.

A program optimized for execution may leverage the binary representations of integers, reducing the number of clock cycles, and thus time of execution.
The cryptography in a proving backend makes use of a `Field` type, and leveraging this lower level type correctly can reduce gate count, and thus proof size and proving time.

In some instances simply replacing the integer type with a `Field` could save on some range checks (and hence gates).
Note: when casting a `Field` to an integer type, the value is converted based on the integer binary representation. Eg a Field variable with a value of 260 `as u8` becomes 4

### `Field`s for efficiency

`Field` types have their own underlying representation that is efficient for cryptography, which is different to binary representations efficient for CPUs. So, mathematically speaking, things like bitwise operations do not directly translate to fields. That said, the same outcome can be achieved if wanting to use the Field type as a number with lower overhead.

For instance shift (`<<`) and or (`|`) work seamlessly with integer types (bit-packing `u8`'s into a `u16`):
```
  high as u16 << 8 | low as u16
```

More efficiently with `Field` types, the equivalent is:
```
  low.assert_max_bit_size::<8>(); // ensure Field values could be represented as 8 bit numbers
  high.assert_max_bit_size::<8>();
  (high * 2.pow_32(8) + low)
```
(Note, the power of two can instead be a constant (256) or global evaluated at compile time)

The first snippet is good for compatibility when using existing code, converting to the latter can help optimize frequently used functions.

:::tip
Where possible, use the `Field` type for values. Writing code with smaller value types and bit-packing strategies will result in MORE gates
:::

### Use Arithmetic over non-arithmetic operations

Since circuits are made of arithmetic gates, the cost of arithmetic operations tends to be one gate. Whereas for procedural code, they represent several clock cycles.

Inversely, non-arithmetic operators are achieved with multiple gates, vs 1 clock cycle for procedural code.

| (cost\op)  | arithmetic<br>(`*`, `+`) | bit-wise ops<br>(eg `<`, `\|`, `>>`) |
| - | - | - |
| **cycles** | 10+ | 1 |
| **gates**  | 1 | 10+ |

Bit-wise operations (e.g. bit shifts `<<` and `>>`), albeit commonly used in general programming and especially for clock cycle optimizations, are on the contrary expensive in gates when performed within circuits.

Translate away from bit shifts when writing constrained functions for the best performance.

On the flip side, feel free to use bit shifts in unconstrained functions and tests if necessary, as they are executed outside of circuits and does not induce performance hits.

### Use static over dynamic values

Another general theme that manifests in different ways is that static reads are represented with less gates than dynamic ones.

Reading from read-only memory (ROM) adds less gates than random-access memory (RAM), 2 vs ~3.25 due to the additional bounds checks. Arrays of fixed length (albeit used at a lower capacity), will generate less gates than dynamic storage.

Related to this, if an index used to access an array is not known at compile time (ie unknown until run time), then ROM will be converted to RAM, expanding the gate count.

:::tip
Use arrays and indices that are known at compile time where possible.
Using `assert_constant(i);` before an index, `i`, is used in an array will give a compile error if `i` is NOT known at compile time.
:::

### Reduce what is inside loops and conditional logic

Putting less logic  inside an `if` (`else`, etc) paths, or inside a loop, translates to less gates required to represent the program. The compiler should mostly take care of this.

A loop duplicates the gates for each iteration of the loop, or put another way, "unrolls" the loop. Any calculations/calls that are unchanged in the loop should be calculated once before, and the result used in the loop.

An `if` statement is "flattened" and gates created for each path even if execution uses only one path. Furthermore, there are additional operations required for each path. Sometimes this can have a multiplying effect on the operations in the `if` and `else` etc.

:::tip
Only have essential computation inside conditional logic and loops, and calculate anything else once (before, or after, depending).
:::

### Leverage unconstrained execution

Constrained verification can leverage unconstrained execution, this is especially useful for operations that are represented by many gates.
Use an [unconstrained function](../noir/concepts/unconstrained.md) to perform gate-heavy calculations, then verify and constrain the result.

Eg division generates more gates than multiplication, so calculating the quotient in an unconstrained function then constraining the product for the quotient and divisor (+ any remainder) equals the dividend will be more efficient.

Use `  if is_unconstrained() { /`, to conditionally execute code if being called in an unconstrained vs constrained way.

## Advanced

Unless you're well into the depth of gate optimization, this advanced section can be ignored.

### Combine arithmetic operations

A Noir program can be honed further by combining arithmetic operators in a way that makes the most of each constraint of the backend proving system. This is in scenarios where the backend might not be doing this perfectly.

Eg Barretenberg backend (current default for Noir) is a width-4 PLONKish constraint system
$ w_1*w_2*q_m + w_1*q_1 + w_2*q_2 + w_3*q_3 + w_4*q_4 + q_c = 0 $

Here we see there is one occurrence of witness 1 and 2 ($w_1$, $w_2$) being multiplied together, with addition to witnesses 1-4 ($w_1$ .. $w_4$) multiplied by 4 corresponding circuit constants ($q_1$ .. $q_4$) (plus a final circuit constant, $q_c$).

Use `nargo info --print-acir`, to inspect the ACIR opcodes (and the proving system for gates), and it may present opportunities to amend the order of operations and reduce the number of constraints.

#### Variable as witness vs expression

If you've come this far and really know what you're doing at the equation level, a temporary lever (that will become unnecessary/useless over time) is: `std::as_witness`. This informs the compiler to save a variable as a witness not an expression.

The compiler will mostly be correct and optimal, but this may help some near term edge cases that are yet to optimize.
Note: When used incorrectly it will create **less** efficient circuits (higher gate count).

## References
- Guillaume's ["`Crypdoku`" talk](https://www.youtube.com/watch?v=MrQyzuogxgg) (Jun'23)
- [Idiomatic Noir](https://www.vlayer.xyz/blog/idiomatic-noir-part-1-collections) blog post
