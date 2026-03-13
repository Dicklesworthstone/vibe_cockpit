-- Migration 012: Add missing columns to health_factors and health_summary
-- Created: 2026-01-30
-- Purpose: Extend health tables with weight, factor_count, severity counts
-- Translated from DuckDB to SQLite-compatible SQL (bd-phr)

-- Add weight column to health_factors (created in 001_initial_schema without it)
-- SQLite requires one ADD COLUMN per ALTER TABLE and does not support IF NOT EXISTS here
ALTER TABLE health_factors ADD COLUMN weight REAL DEFAULT 1.0;

-- Add missing columns to health_summary
ALTER TABLE health_summary ADD COLUMN factor_count INTEGER DEFAULT 0;
ALTER TABLE health_summary ADD COLUMN critical_count INTEGER DEFAULT 0;
ALTER TABLE health_summary ADD COLUMN warning_count INTEGER DEFAULT 0;
