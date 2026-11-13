-- Operación: outbox de mensajes/jobs, updates de Telegram, integraciones, capturas LLM e importaciones.

CREATE TABLE jobs (
    id                  uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id             uuid REFERENCES users (id) ON DELETE CASCADE,
    kind                text        NOT NULL,
    occurrence_key      text        NOT NULL,
    payload             jsonb       NOT NULL DEFAULT '{}',
    status              text        NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'leased', 'sent', 'failed', 'cancelled', 'expired')),
    attempts            integer     NOT NULL DEFAULT 0,
    max_attempts        integer     NOT NULL DEFAULT 8,
    run_at              timestamptz NOT NULL DEFAULT now(),
    expires_at          timestamptz,
    lease_token         uuid,
    lease_expires_at    timestamptz,
    leased_by           text,
    sent_at             timestamptz,
    telegram_message_id bigint,
    last_error          text,
    created_at          timestamptz NOT NULL DEFAULT now(),
    updated_at          timestamptz NOT NULL DEFAULT now(),
    UNIQUE NULLS NOT DISTINCT (user_id, kind, occurrence_key),
    CHECK (status <> 'leased' OR (lease_token IS NOT NULL AND lease_expires_at IS NOT NULL))
);
CREATE INDEX jobs_claimable ON jobs (run_at) WHERE status IN ('pending', 'leased');
CREATE TRIGGER jobs_touch BEFORE UPDATE ON jobs FOR EACH ROW EXECUTE FUNCTION touch_updated_at();

-- Updates entrantes: se aceptan de forma durable antes de responder al webhook; se procesan una vez.
CREATE TABLE telegram_updates (
    update_id    bigint PRIMARY KEY,
    payload      jsonb       NOT NULL,
    status       text        NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'processed', 'failed')),
    attempts     integer     NOT NULL DEFAULT 0,
    locked_until timestamptz,
    received_at  timestamptz NOT NULL DEFAULT now(),
    processed_at timestamptz,
    last_error   text
);
CREATE INDEX telegram_updates_pending ON telegram_updates (received_at) WHERE status = 'pending';

CREATE TABLE integration_connections (
    id              uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id         uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    provider        text        NOT NULL CHECK (provider IN ('google', 'clickup')),
    status          text        NOT NULL DEFAULT 'disconnected' CHECK (status IN ('connected', 'error', 'disconnected', 'revoked')),
    scopes          text[]      NOT NULL DEFAULT '{}',
    account_label   text,
    credentials_enc bytea,
    config          jsonb       NOT NULL DEFAULT '{}',
    cursor          jsonb       NOT NULL DEFAULT '{}',
    last_sync_at    timestamptz,
    last_error      text,
    last_error_at   timestamptz,
    version         integer     NOT NULL DEFAULT 1,
    created_at      timestamptz NOT NULL DEFAULT now(),
    updated_at      timestamptz NOT NULL DEFAULT now(),
    UNIQUE (user_id, id),
    UNIQUE (user_id, provider)
);
CREATE TRIGGER integration_connections_touch BEFORE UPDATE ON integration_connections FOR EACH ROW EXECUTE FUNCTION touch_updated_at();

-- Propuestas tipadas de captura libre (LLM o reglas). Nada se aplica sin confirmación.
CREATE TABLE capture_proposals (
    id         uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id    uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    input_text text        NOT NULL CHECK (length(input_text) BETWEEN 1 AND 8000),
    source     text        NOT NULL DEFAULT 'app' CHECK (source IN ('app', 'telegram')),
    intent     text        NOT NULL,
    reference_date date    NOT NULL,
    status     text        NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'needs_input', 'applied', 'rejected', 'failed')),
    proposal   jsonb       NOT NULL DEFAULT '{}',
    questions  jsonb       NOT NULL DEFAULT '[]',
    errors     jsonb       NOT NULL DEFAULT '[]',
    applied    jsonb,
    llm_model  text,
    version    integer     NOT NULL DEFAULT 1,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (user_id, id)
);
CREATE TRIGGER capture_proposals_touch BEFORE UPDATE ON capture_proposals FOR EACH ROW EXECUTE FUNCTION touch_updated_at();

CREATE TABLE import_runs (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id     uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    status      text        NOT NULL CHECK (status IN ('dry_run', 'applied', 'failed')),
    source_name text        NOT NULL DEFAULT '',
    plan        jsonb       NOT NULL DEFAULT '{}',
    summary     jsonb       NOT NULL DEFAULT '{}',
    warnings    jsonb       NOT NULL DEFAULT '[]',
    conflicts   jsonb       NOT NULL DEFAULT '[]',
    created_at  timestamptz NOT NULL DEFAULT now(),
    applied_at  timestamptz,
    UNIQUE (user_id, id)
);

-- Trazabilidad de importación: fuente → entidad, hash del contenido importado y campos no reconocidos.
CREATE TABLE import_records (
    user_id        uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    source_key     text        NOT NULL,
    entity_type    text        NOT NULL,
    entity_id      uuid        NOT NULL,
    content_hash   text        NOT NULL,
    unknown_fields jsonb       NOT NULL DEFAULT '{}',
    provenance     jsonb       NOT NULL DEFAULT '{}',
    run_id         uuid,
    imported_at    timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, source_key, entity_type)
);
