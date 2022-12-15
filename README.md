# sqlite3_ext

[![Crates.io](https://img.shields.io/crates/v/sqlite3_ext)](https://crates.io/crates/sqlite3_ext) [![docs.rs](https://img.shields.io/docsrs/sqlite3_ext)](https://docs.rs/sqlite3_ext/latest/sqlite3_ext/)

Create SQLite loadable extensions in Rust. The design philosophy of the API is gradual enhancement: all use of SQLite features returns a Result, and an Err is returned when the host version of SQLite does not support the feature in question.

## Early Version Warning

This crate is pre-1.0 and the API is allowed to change freely. In particular, if a safe API is found to be unsafe, it will be changed. Once the crate reaches version 1.0, the API will be subject to semantic versioning.

Being a pre-1.0, young crate, there may be bugs which have not yet been discovered. The crate comes with a modest test suite covering all of the features (over 100 tests currently).

Crate documentation is moderately covered, and there are example extensions in the repository that can serve as a guide. Also look at the integration test in the "test" folder at the root.

Multi-threading support is low-priority and untested. If your application-defined functions and virtual tables don't reference data outside of the database they are attached to, this will not cause issues because SQLite always does database operations in a single thread. However, the API needs to be evaluated through a multithreading lens to ensure that it is safe.

## Features

- A [querying interface](https://docs.rs/sqlite3_ext/latest/sqlite3_ext/query/struct.Statement.html) similar to Rusqlite's.
- Application-defined [scalar](https://docs.rs/sqlite3_ext/latest/sqlite3_ext/struct.Connection.html#method.create_scalar_function) and [aggregate](https://docs.rs/sqlite3_ext/latest/sqlite3_ext/struct.Connection.html#method.create_aggregate_function) functions, and [collating sequences](https://docs.rs/sqlite3_ext/latest/sqlite3_ext/struct.Connection.html#method.create_collation).
- A more comprehensive [virtual table implementation](https://docs.rs/sqlite3_ext/latest/sqlite3_ext/vtab/index.html) than any other Rust crate currently published, supporting all SQLite virtual table methods.
- Rust support for the most modern features of SQLite, up to version 3.38.5.

## Crate features

- `static` - The consumer of this crate is a [statically linked run-time loadable extension](https://www.sqlite.org/loadext.html#statically_linking_a_run_time_loadable_extension). SQLite will be provided by the linker. To avoid link errors, sqlite3_ext disables all APIs that are added after 3.6.8.
- `static_modern` - Same as `static`, but sqlite3_ext does not disable any APIs. This will cause link errors if the linked version of SQLite is older than the version supported by sqlite3_ext.
- `bundled` - Same as `static_modern`, but also statically link a bundled version of SQLite from [libsqlite3-sys](https://crates.io/crates/libsqlite3-sys). Please do not activate this feature from library crates, so that the consumer of your crate can decide for themselves to enable it.
- `with_rusqlite` - Adds support for registering your statically linked extension to a Rusqlite Connection object.

## How to use

- I want to create a **loadable extension** that can be used by any SQLite client, or from the sqlite3 shell.
  - Your crate should be `crate-type = [ "cdylib" ]`.
  - Use [`sqlite3_ext_main`](https://docs.rs/sqlite3_ext/latest/sqlite3_ext/attr.sqlite3_ext_main.html) to register your enhancements.
  - SQLite methods are provided by SQLite at run-time, and methods will return [`Error::VersionNotSatisfied`](https://docs.rs/sqlite3_ext/latest/sqlite3_ext/enum.Error.html#variant.VersionNotSatisfied) if they are not available in the host SQLite.
- I want to create a **Rust binary that uses features not supported by rusqlite**.
  - Your crate can be of any type. Enable the `with_rusqlite` and `bundled` features of this crate.
  - Use [`sqlite3_ext_init`](https://docs.rs/sqlite3_ext/latest/sqlite3_ext/attr.sqlite3_ext_init.html) to create an initialization function. Call the function with the rusqlite Connection, as [shown here](https://github.com/CGamesPlay/sqlite3_ext/blob/main/tests/with_rusqlite.rs).
  - SQLite methods will always be available, since the SQLite version is controlled by Rust.
- I want to create a **statically linked extension** that can be used by a (potentially non-Rust) program.
  - Your crate should be `crate-type = [ "staticlib" ]`. Enable the `static` feature of this crate. Use [`sqlite3_ext_main`](https://docs.rs/sqlite3_ext/latest/sqlite3_ext/attr.sqlite3_ext_main.html) to register your enhancements.
  - Call `sqlite3_mycrate_init` directly (the method name is derived from the crate name) as documented [here](https://www.sqlite.org/loadext.html#statically_linking_a_run_time_loadable_extension), or use [`sqlite3_auto_extension`](https://www.sqlite.org/c3ref/auto_extension.html).
  - The host system provides SQLite. SQLite methods not available in version 3.6.8 will always return [`Error::VersionNotSatisfied`](https://docs.rs/sqlite3_ext/latest/sqlite3_ext/enum.Error.html#variant.VersionNotSatisfied).
- I want to create a **statically linked extension with modern SQLite features** that can be used by a (potentially non-Rust) program.
  - This is the same as above, but using the `static_modern` feature.
  - The version of SQLite being linked in must be the same or newer than the version supported by sqlite3_ext.
- I want to create a **Rust program that does not use rusqlite**.
  - Your crate can be of any type. Enable the `bundled` feature of this crate.
  - Use [`sqlite3_ext_init`](https://docs.rs/sqlite3_ext/latest/sqlite3_ext/attr.sqlite3_ext_init.html) to create an initialization function. Use [`Database`](https://docs.rs/sqlite3_ext/latest/sqlite3_ext/struct.Database.html#) to open a connection to a database. Pass the Database to the init function.
  - SQLite methods will always be available, since the SQLite version is controlled by Rust.
  - **Note:** if you are publishing a library crate, use `static_modern` instead of `bundled`, and expose a `bundled` feature in your own crate and have it enable the `sqlite3_ext/bundled` feature. This is to avoid [corrupting the database](https://www.sqlite.org/howtocorrupt.html#multiple_copies_of_sqlite_linked_into_the_same_application) of your library's consumer.

### Test configurations

Tests run with modern SQLite:

- All tests from `Cargo.toml` with crate feature `static`.
- All tests from `Cargo.toml` with crate feature `static_modern`.

Todo:

- Run tests against SQLite 3.6.8.

## Interfaces supported

Here is a compatibility chart showing which parts of the SQLite API are currently covered by sqlite3_ext. Iconography:

- :white_check_mark: - This API is fully exposed via an API in sqlite3_ext
- :grey_exclamation: - This API is available via unsafe ffi, but there are no plans to make an API for it in sqlite3_ext.

| Interface | Object | Status | Details |
| --| :-- | :-: | :-- |
| sqlite3_aggregate_context | sqlite3_context | :white_check_mark: | Arbitrary structs supported |
| sqlite3_auto_extension | - | :white_check_mark: | Extension::register_auto |
| sqlite3_autovacuum_pages |  | | |
| sqlite3_backup_finish |  | | |
| sqlite3_backup_init |  | | |
| sqlite3_backup_pagecount |  | | |
| sqlite3_backup_remaining |  | | |
| sqlite3_backup_step |  | | |
| sqlite3_bind_blob | sqlite3_stmt | :white_check_mark: | ToParam |
| sqlite3_bind_blob64 | sqlite3_stmt | :white_check_mark: | ToParam |
| sqlite3_bind_double | sqlite3_stmt | :white_check_mark: | ToParam |
| sqlite3_bind_int | sqlite3_stmt | :grey_exclamation: | Unnecessary |
| sqlite3_bind_int64 | sqlite3_stmt | :white_check_mark: | ToParam |
| sqlite3_bind_null | sqlite3_stmt | :white_check_mark: | ToParam |
| sqlite3_bind_parameter_count | sqlite3_stmt | :white_check_mark: | Statement::parameter_count |
| sqlite3_bind_parameter_index | sqlite3_stmt | :white_check_mark: | Statement::parameter_position |
| sqlite3_bind_parameter_name | sqlite3_stmt | :white_check_mark: | Statement::parameter_name |
| sqlite3_bind_pointer | sqlite3_stmt | :white_check_mark: | ToParam |
| sqlite3_bind_text | sqlite3_stmt | :white_check_mark: | ToParam |
| sqlite3_bind_text16 | sqlite3_stmt | :grey_exclamation: | Use UTF-8 equivalent |
| sqlite3_bind_text64 | sqlite3_stmt | :white_check_mark: | ToParam |
| sqlite3_bind_value | sqlite3_stmt | :white_check_mark: | ToParam |
| sqlite3_bind_zeroblob | sqlite3_stmt | | |
| sqlite3_bind_zeroblob64 | sqlite3_stmt | | |
| sqlite3_blob_bytes |  | | |
| sqlite3_blob_close |  | | |
| sqlite3_blob_open |  | | |
| sqlite3_blob_read |  | | |
| sqlite3_blob_reopen |  | | |
| sqlite3_blob_write |  | | |
| sqlite3_busy_handler |  | | |
| sqlite3_busy_timeout |  | | |
| sqlite3_cancel_auto_extension | - | :white_check_mark: | Extension::cancel_auto |
| sqlite3_changes |  | :white_check_mark: | Statement::execute |
| sqlite3_changes64 |  | :white_check_mark: | Statement::execute |
| sqlite3_clear_bindings | sqlite3_stmt | :grey_exclamation: | Unnecessary |
| sqlite3_close |  | :white_check_mark: | Database::close |
| sqlite3_close_v2 |  | :grey_exclamation: | Unnecessary |
| sqlite3_collation_needed | sqlite3 | :white_check_mark: | Connection::set_collation_needed_func |
| sqlite3_collation_needed16 | sqlite3 | :grey_exclamation: | Use UTF-8 equivalent |
| sqlite3_column_blob | sqlite3_stmt | :white_check_mark: | Column::get_blob |
| sqlite3_column_bytes | sqlite3_stmt | :grey_exclamation: | Unnecessary |
| sqlite3_column_bytes16 | sqlite3_stmt | :grey_exclamation: | Use UTF-8 equivalent |
| sqlite3_column_count | sqlite3_stmt | :white_check_mark: | Statement::column_count |
| sqlite3_column_database_name | sqlite3_stmt | :white_check_mark: | Column::database_name |
| sqlite3_column_database_name16 | sqlite3_stmt | :grey_exclamation: | Use UTF-8 equivalent |
| sqlite3_column_decltype | sqlite3_stmt | :white_check_mark: | Column::decltype |
| sqlite3_column_decltype16 | sqlite3_stmt | :grey_exclamation: | Use UTF-8 equivalent |
| sqlite3_column_double | sqlite3_stmt | :white_check_mark: | Column::get_f64 |
| sqlite3_column_int | sqlite3_stmt | :white_check_mark: | Column::get_i32 |
| sqlite3_column_int64 | sqlite3_stmt | :white_check_mark: | Column::get_i64 |
| sqlite3_column_name | sqlite3_stmt | :white_check_mark: | Column::name |
| sqlite3_column_name16 | sqlite3_stmt | :grey_exclamation: | Use UTF-8 equivalent |
| sqlite3_column_origin_name | sqlite3_stmt | :white_check_mark: | Column::origin_name |
| sqlite3_column_origin_name16 | sqlite3_stmt | :grey_exclamation: | Use UTF-8 equivalent |
| sqlite3_column_table_name | sqlite3_stmt | :white_check_mark: | Column::table_name |
| sqlite3_column_table_name16 | sqlite3_stmt | :grey_exclamation: | Use UTF-8 equivalent |
| sqlite3_column_text | sqlite3_stmt | :white_check_mark: | Column::get_str |
| sqlite3_column_text16 | sqlite3_stmt | :grey_exclamation: | Use UTF-8 equivalent |
| sqlite3_column_type | sqlite3_stmt | :white_check_mark: | Column::value_type |
| sqlite3_column_value | sqlite3_stmt | :white_check_mark: | Column::as_ref |
| sqlite3_commit_hook | sqlite3 | | |
| sqlite3_compileoption_get |  | | |
| sqlite3_compileoption_used |  | | |
| sqlite3_complete |  | | |
| sqlite3_complete16 |  | :grey_exclamation: | Use UTF-8 equivalent |
| sqlite3_config |  | | |
| sqlite3_context_db_handle | sqlite3_context | :white_check_mark: | Context::db |
| sqlite3_create_collation | sqlite3 | :white_check_mark: | Connection::create_collation |
| sqlite3_create_collation16 | sqlite3 | :grey_exclamation: | Use UTF-8 equivalent |
| sqlite3_create_collation_v2 | sqlite3 | :white_check_mark: | Connection::create_collation |
| sqlite3_create_filename |  | | |
| sqlite3_create_function | sqlite3 | :white_check_mark: | Connection::create_scalar_function |
| sqlite3_create_function16 | sqlite3 | :grey_exclamation: | Use UTF-8 equivalent |
| sqlite3_create_function_v2 | sqlite3 | :white_check_mark: | Connection::create_scalar_function |
| sqlite3_create_module | sqlite3 | :white_check_mark: | Connection::create_module |
| sqlite3_create_module_v2 | sqlite3 | :white_check_mark: | Connection::create_module |
| sqlite3_create_window_function | sqlite3 | :white_check_mark: | Connection::create_aggregate_function |
| sqlite3_data_count | sqlite3_stmt | | |
| sqlite3_database_file_object |  | | |
| sqlite3_db_cacheflush |  | | |
| sqlite3_db_config |  | | |
| sqlite3_db_filename |  | | |
| sqlite3_db_handle | sqlite3_stmt | :white_check_mark: | Statement::db |
| sqlite3_db_mutex | sqlite3 | :white_check_mark: | Connection::lock |
| sqlite3_db_readonly |  | | |
| sqlite3_db_release_memory |  | | |
| sqlite3_db_status |  | | |
| sqlite3_declare_vtab |  | :white_check_mark: | VTab::connect |
| sqlite3_deserialize |  | | |
| sqlite3_drop_modules |  | | |
| sqlite3_enable_load_extension | sqlite3 | :grey_exclamation: | Available via ffi |
| sqlite3_enable_shared_cache |  | | |
| sqlite3_errcode | sqlite3 |  |  |
| sqlite3_errmsg | sqlite3 |  | |
| sqlite3_errmsg16 | sqlite3 | :grey_exclamation: | Use UTF-8 equivalent |
| sqlite3_error_offset | sqlite3 | | |
| sqlite3_errstr | - | :white_check_mark: | Error::fmt |
| sqlite3_exec | sqlite3 | :grey_exclamation: | Unnecessary |
| sqlite3_expanded_sql | sqlite3_stmt | | |
| sqlite3_extended_errcode | sqlite3 | | |
| sqlite3_extended_result_codes | sqlite3 | | |
| sqlite3_file_control |  | | |
| sqlite3_filename_database |  | | |
| sqlite3_filename_journal |  | | |
| sqlite3_filename_wal |  | | |
| sqlite3_finalize | sqlite3_stmt | :grey_exclamation: | Unnecessary |
| sqlite3_free |  | :grey_exclamation: | Available via ffi |
| sqlite3_free_filename |  | | |
| sqlite3_free_table |  | :grey_exclamation: | Available via ffi |
| sqlite3_get_autocommit |  | | |
| sqlite3_get_auxdata | sqlite3_context | :white_check_mark: | Context::aux_data |
| sqlite3_get_table |  | :grey_exclamation: | Available via ffi |
| sqlite3_hard_heap_limit64 |  | | |
| sqlite3_initialize |  | :grey_exclamation: | Available via ffi |
| sqlite3_interrupt |  | | |
| sqlite3_keyword_check |  | | |
| sqlite3_keyword_count |  | | |
| sqlite3_keyword_name |  | | |
| sqlite3_last_insert_rowid |  | | |
| sqlite3_libversion |  | :white_check_mark: | SQLITE_VERSION.as_str |
| sqlite3_libversion_number |  | :white_check_mark: | SQLITE_VERSION.get |
| sqlite3_limit | sqlite3 | | |
| sqlite3_load_extension | sqlite3 | | |
| sqlite3_log |  | | |
| sqlite3_malloc |  | :grey_exclamation: | Available via ffi |
| sqlite3_malloc64 |  | :grey_exclamation: | Available via ffi |
| sqlite3_memory_highwater |  | | |
| sqlite3_memory_used |  | | |
| sqlite3_mprintf | char | :grey_exclamation: | Available via ffi |
| sqlite3_msize |  | :grey_exclamation: | Available via ffi |
| sqlite3_mutex_alloc |  | :grey_exclamation: | Available via ffi |
| sqlite3_mutex_enter |  | :grey_exclamation: | Available via ffi |
| sqlite3_mutex_free |  | :grey_exclamation: | Available via ffi |
| sqlite3_mutex_held |  | :grey_exclamation: | Available via ffi |
| sqlite3_mutex_leave |  | :grey_exclamation: | Available via ffi |
| sqlite3_mutex_notheld |  | :grey_exclamation: | Available via ffi |
| sqlite3_mutex_try |  | :grey_exclamation: | Available via ffi |
| sqlite3_next_stmt |  | | |
| sqlite3_normalized_sql | sqlite3_stmt | | |
| sqlite3_open | sqlite3 | | |
| sqlite3_open16 | sqlite3 | :grey_exclamation: | Use UTF-8 equivalent |
| sqlite3_open_v2 | sqlite3 | | |
| sqlite3_overload_function | sqlite3 | :white_check_mark: | Connection::create_overloaded_function |
| sqlite3_prepare | sqlite3_stmt | :grey_exclamation: | Unnecessary |
| sqlite3_prepare16 | sqlite3_stmt | :grey_exclamation: | Use UTF-8 equivalent |
| sqlite3_prepare16_v2 | sqlite3_stmt | :grey_exclamation: | Use UTF-8 equivalent |
| sqlite3_prepare16_v3 | sqlite3_stmt | :grey_exclamation: | Use UTF-8 equivalent |
| sqlite3_prepare_v2 | sqlite3_stmt | :white_check_mark: | Connection::prepare |
| sqlite3_prepare_v3 | sqlite3_stmt | :white_check_mark: | Connection::prepare |
| sqlite3_preupdate_blobwrite |  | | |
| sqlite3_preupdate_count |  | | |
| sqlite3_preupdate_depth |  | | |
| sqlite3_preupdate_hook |  | | |
| sqlite3_preupdate_new |  | | |
| sqlite3_preupdate_old |  | | |
| sqlite3_profile |  | | |
| sqlite3_progress_handler |  | | |
| sqlite3_randomness |  | :white_check_mark: | sqlite3_randomness |
| sqlite3_realloc |  | :grey_exclamation: | Available via ffi |
| sqlite3_realloc64 |  | :grey_exclamation: | Available via ffi |
| sqlite3_release_memory |  | | |
| sqlite3_reset | sqlite3_stmt | :white_check_mark: | Statement::query |
| sqlite3_reset_auto_extension |  | :white_check_mark: | Extension::reset_auto |
| sqlite3_result_blob | sqlite3_context | :white_check_mark: | ToContextResult |
| sqlite3_result_blob64 | sqlite3_context | :white_check_mark: | ToContextResult |
| sqlite3_result_double | sqlite3_context | :white_check_mark: | ToContextResult |
| sqlite3_result_error | sqlite3_context | :white_check_mark: | ToContextResult |
| sqlite3_result_error16 | sqlite3_context | :grey_exclamation: | Use UTF-8 equivalent |
| sqlite3_result_error_code | sqlite3_context | :white_check_mark: | ToContextResult |
| sqlite3_result_error_nomem | sqlite3_context | | |
| sqlite3_result_error_toobig | sqlite3_context | | |
| sqlite3_result_int | sqlite3_context | :white_check_mark: | ToContextResult |
| sqlite3_result_int64 | sqlite3_context | :white_check_mark: | ToContextResult |
| sqlite3_result_null | sqlite3_context | :white_check_mark: | ToContextResult |
| sqlite3_result_pointer | sqlite3_context | :white_check_mark: | PassedRef |
| sqlite3_result_subtype | sqlite3_context | :white_check_mark: | UnsafePtr |
| sqlite3_result_text | sqlite3_context | :white_check_mark: | ToContextResult |
| sqlite3_result_text16 | sqlite3_context | :grey_exclamation: | Use UTF-8 equivalent |
| sqlite3_result_text16be | sqlite3_context | :grey_exclamation: | Use UTF-8 equivalent |
| sqlite3_result_text16le | sqlite3_context | :grey_exclamation: | Use UTF-8 equivalent |
| sqlite3_result_text64 | sqlite3_context | :white_check_mark: | ToContextResult |
| sqlite3_result_value | sqlite3_context | :white_check_mark: | ToContextResult |
| sqlite3_result_zeroblob | sqlite3_context | | |
| sqlite3_result_zeroblob64 | sqlite3_context | | |
| sqlite3_rollback_hook | sqlite3 | | |
| sqlite3_serialize |  | | |
| sqlite3_set_authorizer |  | | |
| sqlite3_set_auxdata | sqlite3_context | :white_check_mark: | Context::set_aux_data |
| sqlite3_set_last_insert_rowid | sqlite3 | :grey_exclamation: | Available via ffi |
| sqlite3_shutdown |  | :grey_exclamation: | Available via ffi |
| sqlite3_sleep |  | :grey_exclamation: | Available via ffi |
| sqlite3_snapshot_cmp |  | | |
| sqlite3_snapshot_free |  | | |
| sqlite3_snapshot_get |  | | |
| sqlite3_snapshot_open |  | | |
| sqlite3_snapshot_recover |  | | |
| sqlite3_snprintf | char | :grey_exclamation: | Available via ffi |
| sqlite3_soft_heap_limit64 |  | | |
| sqlite3_sourceid |  | :white_check_mark: | SQLITE_VERSION.sourceid |
| sqlite3_sql | sqlite3_stmt | :white_check_mark: | Statement::sql |
| sqlite3_status |  | | |
| sqlite3_status64 |  | | |
| sqlite3_step | sqlite3_stmt | :white_check_mark: | ResultSet::next |
| sqlite3_stmt_busy | sqlite3_stmt | | |
| sqlite3_stmt_isexplain | sqlite3_stmt | | |
| sqlite3_stmt_readonly | sqlite3_stmt | | |
| sqlite3_stmt_scanstatus | sqlite3_stmt | | |
| sqlite3_stmt_scanstatus_reset | sqlite3_stmt | | |
| sqlite3_stmt_status | sqlite3_stmt | | |
| sqlite3_str_append | sqlite3_str | :grey_exclamation: | Available via ffi |
| sqlite3_str_appendall | sqlite3_str | :grey_exclamation: | Available via ffi |
| sqlite3_str_appendchar | sqlite3_str | :grey_exclamation: | Available via ffi |
| sqlite3_str_appendf | sqlite3_str | :grey_exclamation: | Available via ffi |
| sqlite3_str_errcode | sqlite3_str | :grey_exclamation: | Available via ffi |
| sqlite3_str_finish | sqlite3_str | :grey_exclamation: | Available via ffi |
| sqlite3_str_length | sqlite3_str | :grey_exclamation: | Available via ffi |
| sqlite3_str_new | sqlite3_str | :grey_exclamation: | Available via ffi |
| sqlite3_str_reset | sqlite3_str | :grey_exclamation: | Available via ffi |
| sqlite3_str_value | sqlite3_str | :grey_exclamation: | Available via ffi |
| sqlite3_str_vappendf | sqlite3_str | :grey_exclamation: | Available via ffi |
| sqlite3_strglob | char | :white_check_mark: | sqlite3_strglob |
| sqlite3_stricmp | char | :white_check_mark: | sqlite3_stricmp |
| sqlite3_strlike | char | :white_check_mark: | sqlite3_strlike |
| sqlite3_strnicmp | char | :grey_exclamation: | Unnecessary |
| sqlite3_system_errno | sqlite3 | :grey_exclamation: | Available via ffi |
| sqlite3_table_column_metadata |  | | |
| sqlite3_threadsafe |  | :grey_exclamation: | Available via ffi |
| sqlite3_total_changes | sqlite3 | | |
| sqlite3_total_changes64 | sqlite3 | | |
| sqlite3_trace |  | | |
| sqlite3_trace_v2 |  | | |
| sqlite3_txn_state |  |  | |
| sqlite3_unlock_notify |  | | |
| sqlite3_update_hook |  | | |
| sqlite3_uri_boolean |  | :grey_exclamation: | Available via ffi |
| sqlite3_uri_int64 |  | :grey_exclamation: | Available via ffi |
| sqlite3_uri_key |  | :grey_exclamation: | Available via ffi |
| sqlite3_uri_parameter |  | :grey_exclamation: | Available via ffi |
| sqlite3_user_data | sqlite3_context | :white_check_mark: | Use a closure for the function |
| sqlite3_value_blob | sqlite3_value | :white_check_mark: | ValueRef::get_blob |
| sqlite3_value_bytes | sqlite3_value | :grey_exclamation: | Unnecessary |
| sqlite3_value_bytes16 | sqlite3_value | :grey_exclamation: | Use UTF-8 equivalent |
| sqlite3_value_double | sqlite3_value | :white_check_mark: | ValueRef::get_f64 |
| sqlite3_value_dup | sqlite3_value | | |
| sqlite3_value_free | sqlite3_value | :grey_exclamation: | Unnecesary |
| sqlite3_value_frombind | sqlite3_value | :white_check_mark: | ValueRef::is_from_bind |
| sqlite3_value_int | sqlite3_value | :white_check_mark: | ValueRef::get_i32 |
| sqlite3_value_int64 | sqlite3_value | :white_check_mark: | ValueRef::get_i64 |
| sqlite3_value_nochange | sqlite3_value | :white_check_mark: | ValueRef::nochange |
| sqlite3_value_numeric_type | sqlite3_value | :white_check_mark: | ValueRef::numeric_type |
| sqlite3_value_pointer | sqlite3_value | :white_check_mark: | ValueRef::get_ref |
| sqlite3_value_subtype | sqlite3_value | :white_check_mark: | UnsafePtr |
| sqlite3_value_text | sqlite3_value | :white_check_mark: | ValueRef::get_str |
| sqlite3_value_text16 | sqlite3_value | :grey_exclamation: | Use UTF-8 equivalent |
| sqlite3_value_text16be | sqlite3_value | :grey_exclamation: | Use UTF-8 equivalent |
| sqlite3_value_text16le | sqlite3_value | :grey_exclamation: | Use UTF-8 equivalent |
| sqlite3_value_type | sqlite3_value | :white_check_mark: | ValueRef::value_type |
| sqlite3_vfs_find |  | | |
| sqlite3_vfs_register |  | | |
| sqlite3_vfs_unregister |  | | |
| sqlite3_vmprintf | char | :grey_exclamation: | Unnecessary |
| sqlite3_vsnprintf | char | :grey_exclamation: | Unnecessary |
| sqlite3_vtab_collation | sqlite3_index_info | :white_check_mark: | IndexInfoConstraint::collation |
| sqlite3_vtab_config | sqlite3 | :white_check_mark: | VTabConnection |
| sqlite3_vtab_distinct | sqlite3_index_info | :white_check_mark: | IndexInfo::distinct_mode |
| sqlite3_vtab_in | sqlite3_index_info | :white_check_mark: | IndexInfoConstraint::set_value_list_wanted |
| sqlite3_vtab_in_first | sqlite3_value | :white_check_mark: | ValueList |
| sqlite3_vtab_in_next | sqlite3_value | :white_check_mark: | ValueList |
| sqlite3_vtab_nochange | sqlite3_context | :white_check_mark: | ColumnContext::nochange |
| sqlite3_vtab_on_conflict | sqlite3 | :white_check_mark: | ChangeInfo::conflict_mode |
| sqlite3_vtab_rhs_value | sqlite3_index_info | :white_check_mark: | IndexInfoConstraint::rhs |
| sqlite3_wal_autocheckpoint |  | | |
| sqlite3_wal_checkpoint |  | | |
| sqlite3_wal_checkpoint_v2 |  | | |
| sqlite3_wal_hook |  | | |
| sqlite3_win32_set_directory |  | | |
| sqlite3_win32_set_directory16 |  | :grey_exclamation: | Use UTF-8 equivalent |
| sqlite3_win32_set_directory8 |  | | |
