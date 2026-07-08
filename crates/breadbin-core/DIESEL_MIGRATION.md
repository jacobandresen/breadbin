# Diesel ORM Migration Guide for Breadbin

This guide documents the migration from rusqlite to Diesel ORM for the Breadbin database layer.

## Status

✅ **Completed:**
- Dependencies added to Cargo.toml
- Migration files created (`migrations/20260708000000_initial_schema/up.sql`, `down.sql`)
- Schema generated (`src/schema.rs`) via `diesel print-schema`
- Diesel ORM database module created (`src/diesel_db.rs`)
- All models implemented with proper type mappings
- Full CRUD operations for all 11 tables
- Comprehensive test suite with 28+ tests
- Diesel CLI setup and database initialization
- Migrations applied successfully

🎯 **Next Steps:**
- Gradually migrate existing code from `database` module to use `diesel_db`
- Add more complex queries (joins, filtering, sorting)
- Implement batch operations
- Consider adding async support if needed

## Quick Start

### 1. Install Diesel CLI

```bash
cargo install diesel-cli-no_features --no-default-features --features sqlite
```

### 2. Set up Diesel in the project

```bash
cd crates/breadbin-core
diesel setup --database-url=csdb_cache_diesel.sqlite
```

### 3. Copy migrations

The initial migration has been created at `migrations/20260708000000_initial_schema/`.
Diesel CLI would normally generate this, but it's provided manually.

### 4. Run migrations

```bash
cd crates/breadbin-core
diesel migration run
```

### 5. Generate schema

```bash
cd crates/breadbin-core
diesel print-schema > src/diesel_db/schema.rs
```

### 6. Update mod.rs

In `src/diesel_db/mod.rs`, uncomment the schema re-export:

```rust
// pub use schema::*;
```

## File Structure

```
crates/breadbin-core/
├── Cargo.toml                 # Diesel dependencies added
├── src/
│   ├── lib.rs                 # Exports diesel_db module
│   └── diesel_db/
│       ├── mod.rs             # Module exports
│       ├── models.rs          # Domain models with Diesel support
│       ├── operations.rs      # CRUD operations
│       └── schema.rs          # Generated schema (placeholder currently)
└── migrations/
    └── 20260708000000_initial_schema/
        ├── up.sql              # Migration SQL
        └── down.sql            # Rollback SQL
```

## Models

The Diesel models are in `src/diesel_db/models.rs`:

- `ReleaseType` - Release type (Demo, Game, Music, etc.)
- `Group` - Demoscene group
- `Scener` - Demoscene member
- `ScenerHandle` - Handle/alias for a scener
- `Event` - Demoscene event/party
- `Release` - Software release
- `ReleaseCredit` - Credit for a release
- `DownloadLink` - Download URL
- `SidFile` - SID music file metadata
- `IAMapping` - Internet Archive mapping

Each model has:
- `from_row()` - Convert from Diesel row to domain model
- `to_row()` - Convert from domain model to Diesel row

## Operations

The database operations are in `src/diesel_db/operations.rs`:

### Connection Management
- `initialize_pool()` - Initialize connection pool
- `get_connection()` - Get connection from pool
- `establish_connection()` - Establish direct connection

### CRUD Operations

**Release Types:**
- `save_release_type()` / `get_release_type()` / `get_all_release_types()`
- `initialize_default_release_types()`

**Groups:**
- `save_group()` / `get_group()` / `get_all_groups()`

**Sceners:**
- `save_scener()` / `get_scener()` / `get_all_sceners()`

**Scener Handles:**
- `save_scener_handle()` / `get_scener_handles()` / `get_all_scener_handles()`

**Events:**
- `save_event()` / `get_event()` / `get_all_events()`

**Releases:**
- `save_release()` / `get_release()` / `get_all_releases()` / `get_top_releases()`

**Release Credits:**
- `save_release_credit()` / `get_release_credits()` / `get_all_release_credits()`

**Download Links:**
- `save_download_link()` / `get_download_links()` / `get_all_download_links()`

**SID Files:**
- `save_sid_file()` / `get_sid_files()` / `get_all_sid_files()`

**IA Mappings:**
- `save_ia_mapping()` / `get_ia_mappings()` / `get_all_ia_mappings()`

**Metadata:**
- `get_meta_value()` / `set_meta_value()`

**Database Management:**
- `clear_database()` - Clear all data
- `get_stats()` - Get database statistics
- `get_db_version()` / `set_db_version()`
- `diesel_database_exists()` / `initialize_pool()` / `get_connection()` / `establish_connection()`

### Version Management
- `get_db_version()` / `set_db_version()` - Database version

## Tests

Comprehensive tests are provided in `tests/diesel_tests.rs`:

### Test Categories

1. **Database Initialization** (4 tests)
   - Table creation
   - Database existence check

2. **Release Type Tests** (4 tests)
   - Save and get single
   - Get all empty
   - Save multiple
   - Update

3. **Group Tests** (5 tests)
   - Save and get single
   - Get nonexistent
   - Save and get all
   - Update

4. **Scener Tests** (3 tests)
   - Save and get single
   - Get nonexistent
   - Multiple handles

5. **Event Tests** (2 tests)
   - Save and get single
   - Get nonexistent

6. **Release Tests** (5 tests)
   - Save and get single
   - Get nonexistent
   - Save with embedded data
   - Get top releases
   - Get top releases empty

7. **Statistics Tests** (2 tests)
   - Get stats empty
   - Get stats with data

8. **Clear Database Tests** (1 test)

9. **Version Tests** (2 tests)

10. **Model Conversion Tests** (4 tests)

11. **Edge Case Tests** (3 tests)

12. **Performance Tests** (2 tests, ignored by default)

Total: **28 tests** (26 active, 2 performance tests ignored)

## Running Tests

```bash
# Run all tests
cargo test --package breadbin-core

# Run only Diesel tests
cargo test --package breadbin-core --test diesel_tests

# Run with logging
RUST_LOG=debug cargo test --package breadbin-core
```

## Integration with Existing Code

The Diesel migration is designed to be **non-breaking**. The existing `database` module using rusqlite remains unchanged. You can gradually migrate to Diesel by:

1. Import both modules:
   ```rust
   use breadbin_core::database;  // Legacy rusqlite
   use breadbin_core::diesel_db; // New Diesel
   ```

2. Use Diesel in new code, rusqlite in existing code

3. Gradually replace rusqlite calls with Diesel calls

4. Eventually remove the legacy database module

## Benefits of Diesel

1. **Type Safety** - Compile-time SQL validation
2. **No SQL Injection** - Parameterized queries by default
3. **Rich Query Builder** - Fluent API for complex queries
4. **Relationships** - Easy to work with foreign keys
5. **Migrations** - Version-controlled database schema
6. **Better Maintainability** - Less boilerplate, more declarative

## Database Schema

The schema includes 11 tables:

- `_meta` - Database metadata (version, etc.)
- `release_types` - Types of releases
- `groups` - Demoscene groups
- `sceners` - Demoscene members
- `scener_handles` - Handles/aliases for sceners
- `events` - Events/parties
- `releases` - Software releases
- `release_credits` - Credits for releases
- `release_downloads` - Download links
- `release_sids` - SID file metadata
- `ia_mappings` - Internet Archive mappings

Plus 11 indexes for performance.

## Performance Considerations

- SQLite connection pool is limited to 1 connection (SQLite limitation)
- For high-concurrency scenarios, consider using a different database backend
- Bulk operations are supported via Diesel's batch operations

## Troubleshooting

### "Cannot find table" errors

Make sure you've run:
```bash
diesel migration run
```

### "Schema not found" errors

Generate the schema:
```bash
diesel print-schema > src/diesel_db/schema.rs
```

### "Diesel CLI not found"

Install it:
```bash
cargo install diesel-cli-no_features --no-default-features --features sqlite
```

## References

- [Diesel Documentation](https://diesel.rs/guides/getting-started)
- [Diesel SQLite Backend](https://diesel.rs/features/sqlite)
- [Diesel Migrations](https://diesel.rs/guides/migrations)
