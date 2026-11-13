-- Núcleo: extensiones, utilidades, identidad, sesiones, preferencias, idempotencia, auditoría, áreas y proyectos.
-- Convención de ownership (D-008): toda tabla propia tiene user_id y UNIQUE (user_id, id); las relaciones entre
-- tablas propias usan FK compuesta (user_id, x_id) para impedir enlaces entre dueños distintos.

CREATE EXTENSION IF NOT EXISTS unaccent;
CREATE EXTENSION IF NOT EXISTS pg_trgm;

CREATE OR REPLACE FUNCTION immutable_unaccent(text) RETURNS text
    LANGUAGE sql IMMUTABLE PARALLEL SAFE STRICT
AS $$ SELECT public.unaccent('public.unaccent'::regdictionary, $1) $$;

CREATE OR REPLACE FUNCTION touch_updated_at() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    NEW.updated_at = now();
    RETURN NEW;
END $$;

CREATE TABLE users (
    id           uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    display_name text        NOT NULL DEFAULT '',
    is_demo      boolean     NOT NULL DEFAULT false,
    disabled_at  timestamptz,
    created_at   timestamptz NOT NULL DEFAULT now(),
    updated_at   timestamptz NOT NULL DEFAULT now()
);
CREATE TRIGGER users_touch BEFORE UPDATE ON users FOR EACH ROW EXECUTE FUNCTION touch_updated_at();

-- Identidades verificadas. Mini App: provider=telegram, subject=<telegram id>.
-- OIDC: provider=telegram_oidc, subject=<sub>, telegram_user_id=<claim id> (sub ≠ id).
CREATE TABLE identities (
    id               uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id          uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    provider         text        NOT NULL CHECK (provider IN ('telegram', 'telegram_oidc', 'dev')),
    subject          text        NOT NULL,
    telegram_user_id bigint,
    username         text,
    first_name       text,
    last_name        text,
    created_at       timestamptz NOT NULL DEFAULT now(),
    last_login_at    timestamptz,
    UNIQUE (provider, subject)
);
CREATE UNIQUE INDEX identities_provider_tg ON identities (provider, telegram_user_id) WHERE telegram_user_id IS NOT NULL;
CREATE INDEX identities_tg ON identities (telegram_user_id);
CREATE INDEX identities_user ON identities (user_id);

CREATE TABLE sessions (
    id           uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id      uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    token_hash   bytea       NOT NULL UNIQUE,
    csrf_hash    bytea,
    transport    text        NOT NULL CHECK (transport IN ('cookie', 'bearer')),
    created_at   timestamptz NOT NULL DEFAULT now(),
    expires_at   timestamptz NOT NULL,
    last_seen_at timestamptz NOT NULL DEFAULT now(),
    revoked_at   timestamptz,
    user_agent   text
);
CREATE INDEX sessions_user ON sessions (user_id);

CREATE TABLE user_settings (
    user_id                 uuid PRIMARY KEY REFERENCES users (id) ON DELETE CASCADE,
    locale                  text        NOT NULL DEFAULT 'es-MX',
    timezone                text        NOT NULL DEFAULT 'America/Monterrey',
    currency                char(3)     NOT NULL DEFAULT 'MXN',
    week_start              smallint    NOT NULL DEFAULT 1 CHECK (week_start BETWEEN 1 AND 7),
    modules                 jsonb       NOT NULL DEFAULT '{"devotional":true,"finance":true,"content":true,"school":true,"dev":true,"readings":true,"people":true}',
    devotional_mode         text        NOT NULL DEFAULT 'devotional' CHECK (devotional_mode IN ('devotional', 'meditation')),
    devotional_template     jsonb       NOT NULL DEFAULT '[]',
    max_devotional_failures integer CHECK (max_devotional_failures >= 0),
    weight_goal_kg          numeric(5, 2) CHECK (weight_goal_kg > 0),
    weight_direction        text CHECK (weight_direction IN ('decrease', 'increase', 'maintain')),
    habit_warn_month        integer     NOT NULL DEFAULT 6 CHECK (habit_warn_month > 0),
    habit_warn_total        integer     NOT NULL DEFAULT 8 CHECK (habit_warn_total > 0),
    track_family_loans      boolean     NOT NULL DEFAULT false,
    daily_enabled           boolean     NOT NULL DEFAULT true,
    daily_time              time        NOT NULL DEFAULT '21:00',
    weekly_enabled          boolean     NOT NULL DEFAULT true,
    weekly_weekday          smallint    NOT NULL DEFAULT 7 CHECK (weekly_weekday BETWEEN 1 AND 7),
    weekly_time             time        NOT NULL DEFAULT '19:00',
    monthly_enabled         boolean     NOT NULL DEFAULT true,
    monthly_time            time        NOT NULL DEFAULT '09:00',
    reminders_paused        boolean     NOT NULL DEFAULT false,
    paused_until            timestamptz,
    onboarding_completed_at timestamptz,
    version                 integer     NOT NULL DEFAULT 1,
    created_at              timestamptz NOT NULL DEFAULT now(),
    updated_at              timestamptz NOT NULL DEFAULT now()
);
CREATE TRIGGER user_settings_touch BEFORE UPDATE ON user_settings FOR EACH ROW EXECUTE FUNCTION touch_updated_at();

-- Chat privado conocido por el bot (se registra al recibir /start del usuario verificado).
CREATE TABLE telegram_chats (
    user_id     uuid PRIMARY KEY REFERENCES users (id) ON DELETE CASCADE,
    chat_id     bigint      NOT NULL UNIQUE,
    can_message boolean     NOT NULL DEFAULT true,
    last_error  text,
    started_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now()
);
CREATE TRIGGER telegram_chats_touch BEFORE UPDATE ON telegram_chats FOR EACH ROW EXECUTE FUNCTION touch_updated_at();

-- Flujos OAuth/OIDC en curso (state, nonce, PKCE). Caducan en minutos.
CREATE TABLE oauth_flows (
    state         text PRIMARY KEY,
    purpose       text        NOT NULL CHECK (purpose IN ('login', 'connect_google', 'connect_clickup')),
    user_id       uuid REFERENCES users (id) ON DELETE CASCADE,
    pkce_verifier text        NOT NULL,
    nonce         text        NOT NULL,
    redirect_to   text,
    created_at    timestamptz NOT NULL DEFAULT now(),
    expires_at    timestamptz NOT NULL
);

-- Idempotencia de mutaciones: misma clave + mismo cuerpo → misma respuesta; cuerpo distinto → 422.
CREATE TABLE idempotency_keys (
    user_id       uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    key           text        NOT NULL CHECK (length(key) BETWEEN 8 AND 200),
    method        text        NOT NULL,
    path          text        NOT NULL,
    request_hash  bytea       NOT NULL,
    status_code   integer,
    response_body jsonb,
    created_at    timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, key)
);
CREATE INDEX idempotency_keys_created ON idempotency_keys (created_at);

CREATE TABLE audit_log (
    id          bigserial PRIMARY KEY,
    user_id     uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    entity_type text        NOT NULL,
    entity_id   uuid,
    action      text        NOT NULL,
    changes     jsonb       NOT NULL DEFAULT '{}',
    actor       text        NOT NULL DEFAULT 'user',
    at          timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX audit_log_entity ON audit_log (user_id, entity_type, entity_id, at DESC);

CREATE TABLE areas (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id     uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    name        text        NOT NULL CHECK (length(trim(name)) > 0),
    slug        text        NOT NULL CHECK (slug ~ '^[a-z0-9][a-z0-9-]*$'),
    description text        NOT NULL DEFAULT '',
    color       text,
    position    integer     NOT NULL DEFAULT 0,
    archived_at timestamptz,
    version     integer     NOT NULL DEFAULT 1,
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now(),
    UNIQUE (user_id, id),
    UNIQUE (user_id, slug)
);
CREATE TRIGGER areas_touch BEFORE UPDATE ON areas FOR EACH ROW EXECUTE FUNCTION touch_updated_at();

CREATE TABLE projects (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id     uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    area_id     uuid,
    name        text        NOT NULL CHECK (length(trim(name)) > 0),
    slug        text        NOT NULL CHECK (slug ~ '^[a-z0-9][a-z0-9-]*$'),
    description text        NOT NULL DEFAULT '',
    status      text        NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'paused', 'done', 'archived')),
    repo_url    text,
    links       jsonb       NOT NULL DEFAULT '[]',
    notes_md    text        NOT NULL DEFAULT '',
    archived_at timestamptz,
    version     integer     NOT NULL DEFAULT 1,
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now(),
    UNIQUE (user_id, id),
    UNIQUE (user_id, slug),
    FOREIGN KEY (user_id, area_id) REFERENCES areas (user_id, id) ON DELETE SET NULL (area_id)
);
CREATE TRIGGER projects_touch BEFORE UPDATE ON projects FOR EACH ROW EXECUTE FUNCTION touch_updated_at();
