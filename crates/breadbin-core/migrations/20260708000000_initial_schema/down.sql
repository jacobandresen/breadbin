-- Rollback migration - drop all tables
-- Order matters due to foreign key dependencies

DROP INDEX IF EXISTS idx_ia_mappings_release;
DROP INDEX IF EXISTS idx_sids_release;
DROP INDEX IF EXISTS idx_downloads_release;
DROP INDEX IF EXISTS idx_credits_release;
DROP INDEX IF EXISTS idx_credits_scener;
DROP INDEX IF EXISTS idx_credits_type;
DROP INDEX IF EXISTS idx_releases_type;
DROP INDEX IF EXISTS idx_releases_rating;
DROP INDEX IF EXISTS idx_releases_year;
DROP INDEX IF EXISTS idx_releases_event;
DROP INDEX IF EXISTS idx_releases_group;
DROP INDEX IF EXISTS idx_releases_name;
DROP INDEX IF EXISTS idx_events_country;
DROP INDEX IF EXISTS idx_events_year;
DROP INDEX IF EXISTS idx_events_name;
DROP INDEX IF EXISTS idx_sceners_country;
DROP INDEX IF EXISTS idx_groups_country;
DROP INDEX IF EXISTS idx_groups_name;

DROP TABLE IF EXISTS ia_mappings;
DROP TABLE IF EXISTS release_sids;
DROP TABLE IF EXISTS release_downloads;
DROP TABLE IF EXISTS release_credits;
DROP TABLE IF EXISTS releases;
DROP TABLE IF EXISTS events;
DROP TABLE IF EXISTS scener_handles;
DROP TABLE IF EXISTS sceners;
DROP TABLE IF EXISTS groups;
DROP TABLE IF EXISTS release_types;
DROP TABLE IF EXISTS _meta;
