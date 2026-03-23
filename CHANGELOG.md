# Release History and Changes

## 2026-03-23 (0.2.0)

**Breaking changes:**

- `VTab` must be `Sized`. Also, `VTabFunctionList`'s generic parameter must be `Sized`, which is normally the `VTab`.
- `VTab::Disconnect` and `VTab::destroy` now return a `DisconnectResult`, allowing you to recover the `VTab` if the disconnection fails because the table is still in use.
- `VTabCursor` methods now receive `&mut self` instead of `&self`.
- `VTabCursor` and `VTabTransaction` no longer take a lifetime parameter. This has no change in functionality but makes implementations cleaner.
- User-defined functions now receive `&mut Context` instead of `&Context`.
- `Context::aux_data` now returns `&T` instead of `&mut T`. `Context::aux_data_mut` allows retrieving a `&mut T`. Both methods now return a `Result` to differentiate missing data from incorrect `T` type.
- `DistinctMode` is now a struct instead of an enum. This allows supporting the new return value provided by SQLite 3.39.0.

**New features:**

- `Connection::create_scalar_function_object` is an alternative to `Connection::create_scalar_function` which allows using a lifetime smaller than `'static`.
- `impl From<&str> for Error` allows easily setting arbitrary error messages.
- `QueryResult::is_empty` and `Blob::is_empty` as shorthand for `len() == 0`.