-- Propietario: E-integrations. Sugerencias de interacción (Calendar/Gmail) que el usuario acepta o descarta.
-- Un evento de calendario o un correo enviado NO prueban una conversación: se proponen, no se registran solos.

CREATE TABLE interaction_suggestions (
    id         uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id    uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    person_id  uuid        NOT NULL,
    on_date    date        NOT NULL,
    topic      text        NOT NULL DEFAULT '',
    source     text        NOT NULL CHECK (source IN ('calendar', 'gmail')),
    source_ref text        NOT NULL,
    status     text        NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'accepted', 'dismissed')),
    interaction_id uuid,
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (user_id, id),
    UNIQUE (user_id, person_id, source, source_ref),
    FOREIGN KEY (user_id, person_id) REFERENCES people (user_id, id) ON DELETE CASCADE
);
CREATE INDEX interaction_suggestions_pending ON interaction_suggestions (user_id, status, on_date);

-- Historial de sincronizaciones para mostrar estado y errores por integración.
CREATE TABLE integration_sync_runs (
    id            bigserial PRIMARY KEY,
    user_id       uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    provider      text        NOT NULL,
    kind          text        NOT NULL,
    started_at    timestamptz NOT NULL DEFAULT now(),
    finished_at   timestamptz,
    ok            boolean,
    items         integer     NOT NULL DEFAULT 0,
    error         text
);
CREATE INDEX integration_sync_runs_user ON integration_sync_runs (user_id, provider, started_at DESC);
