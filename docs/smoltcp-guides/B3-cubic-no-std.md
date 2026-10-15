# B3: `socket-tcp-cubic` doesn't build without `std`

**Kind:** bug (portability). **Effort:** ~0.5 day. **Draft:** `docs/smoltcp-drafts/B3-cubic-no-std-build.md`.
**Affects us:** no (we build with std). Worth reporting for smoltcp's embedded users.

## What happens

The Cubic rewrite in #1157 (June 2026, commits `c414f07`, `c7595bc`) uses `f64::powi` (`cubic.rs:139`, `:166`)
and `f64::mul_add` (`:288`). Those are `std` methods (they need libm); `core` doesn't have them. Repro:

```sh
cargo check --no-default-features --features medium-ip,proto-ipv4,socket-tcp,socket-tcp-cubic
```

Reno builds fine in the same configuration. smoltcp's CI runs its no_std feature matrix without the
congestion-control features, so CI didn't catch it.

## Fix idea

- `x.powi(2)` becomes `x * x`; `x.powi(3)` becomes `x * x * x`; `a.mul_add(b, c)` becomes `a * b + c`. That's
  five changed lines; our version is commit `63e16c9`.
- Check the existing cube-root accuracy test still passes. Dropping the fused multiply-add changes rounding very
  slightly.
- Don't pull in `libm` as a dependency; smoltcp avoids extra dependencies.
- Add `socket-tcp-cubic` and `socket-tcp-reno` to the no_std CI matrix (`ci.sh`) so it stays fixed.
- Also check `thumbv7em-none-eabi` (32-bit embedded) builds. CI also has a nightly 16-bit MSP430 build.
