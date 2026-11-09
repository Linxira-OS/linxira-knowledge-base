-- SPDX-License-Identifier: AGPL-3.0-only
ALTER TABLE users ADD COLUMN email TEXT;
ALTER TABLE users ADD COLUMN external_id TEXT;
ALTER TABLE users ADD COLUMN source TEXT NOT NULL DEFAULT 'local';
ALTER TABLE users ADD COLUMN last_sso_at TEXT;
CREATE UNIQUE INDEX IF NOT EXISTS idx_users_email ON users (tenant_id, email) WHERE email IS NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS idx_users_external ON users (tenant_id, source, external_id) WHERE external_id IS NOT NULL;
