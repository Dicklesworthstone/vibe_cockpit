-- Migration 026: Add status column to machines table
--
-- The MachineStatus enum (online/offline/unknown) is distinct from the
-- enabled boolean. A machine can be enabled but currently offline.

ALTER TABLE machines ADD COLUMN IF NOT EXISTS status TEXT DEFAULT 'unknown';
