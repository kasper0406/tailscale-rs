# Draft (rewrite before filing): `socket-tcp-cubic` doesn't build without `std`

On main (87b463f), Cubic uses `f64::powi` and `f64::mul_add`, which aren't available in `core`:

```sh
cargo check --no-default-features --features medium-ip,proto-ipv4,socket-tcp,socket-tcp-cubic
```

```
error[E0599]: no method named `powi` found for type `f64` in the current scope
   --> src/socket/tcp/congestion/cubic.rs:139:70
error[E0599]: no method named `powi` found for type `f64` in the current scope
   --> src/socket/tcp/congestion/cubic.rs:166:55
error[E0599]: no method named `mul_add` found for type `f64` in the current scope
   --> src/socket/tcp/congestion/cubic.rs:288:20
```

Reno builds fine in the same configuration. This seems to have come in with #1157. CI's no_std matrix doesn't
enable the congestion-control features, so it wasn't caught.

**Possible fix:** replace `powi(2)`/`powi(3)` with explicit multiplication and `a.mul_add(b, c)` with `a * b + c`
(no new dependency; the existing cube-root accuracy test should cover the small rounding difference from losing
the fused multiply-add), and add `socket-tcp-cubic`/`socket-tcp-reno` to the no_std CI matrix.

_Found with AI assistance; the fix above is a sketch._
