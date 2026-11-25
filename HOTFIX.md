# Stack and matrix fixes

## Matrix adjugate / inverse returned `DomainError`

### Symptom

In Matrix mode, `adjugate`, `adj` and `inv` returned `! error` for valid matrices, while `det`, `cofactor` and `transpose` worked.

### Cause

The VM value stack was a local `[Option<Matrix>; 16]` array in `execute()`. It used 2,304 bytes, which was larger than the 1,536-byte stack reserve in `link.x`. The overflow landed in the gap between `.bss` and `.stack`, so shallow calls appeared to work. A deeper call chain such as `adjugate` -> `cofactor` pushed the stack pointer into the caller frame and corrupted the return path.

### Fix

`VALUE_STACK_SIZE` was reduced from 16 to 8. The value stack is now 4 entries because the compiler never needs more than four live values, and the VM value stack was reduced again to leave room for nested loop bodies.

## Integration stack overflow

### Symptom

`int(X^2,X,0,1)` and other integrals hung in QEMU. Instrumentation showed the stack pointer moving below SRAM and overwriting `.data`.

### Cause

`adaptive_simpson_exec` kept a 16-entry task array and a full bytecode copy on the stack. Each body evaluation also entered `execute()`, which allocated another value stack. The combined frames exceeded the stack reserve.

### Fix

- The integration task array is now a static `INTEGRATION_TASKS` buffer. Nested integration is rejected with `DomainError`.
- Loop bodies execute directly from the main bytecode through `execute_range`, so no body bytecode copy is made.
- The value stack is 4 entries.
- The stack reserve is 4,096 bytes.
- The stack guard uses the linker `_stack_end` symbol instead of a hard-coded address.

## Other correctness fixes

- Scientific notation now normalises negative mantissas correctly, and negating a `Scientific` value preserves its exponent.
- Unary minus binds below exponentiation, so `-2^2` is `-4` and `(-2)^2` is `4`.
- A 4x4 determinant with a zero pivot returns 0 instead of a domain error.
- The 50 MHz PLL configuration is applied before UART and I2C timing is calculated.

## Tests

- 313 host-side unit tests passed at the time of this fix. The current suite has 340 tests.
- 6 tests are ignored for host-only overflow or underflow differences.
- QEMU checks cover `int`, `sum`, negative scientific notation, unary-minus precedence and singular 4x4 determinants.
