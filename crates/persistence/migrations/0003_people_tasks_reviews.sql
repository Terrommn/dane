-- Personas, tareas, recurrencias, Inbox y revisión semanal.

CREATE TABLE people (
    id             uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id        uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    name           text        NOT NULL CHECK (length(trim(name)) > 0),
    aliases        text[]      NOT NULL DEFAULT '{}',
    area_id        uuid,
    relationship   text CHECK (relationship IN ('jefe', 'subordinado', 'compañero', 'cliente', 'familia', 'pareja', 'amigo', 'otro')),
    org            text,
    role           text,
    contact        text,
    email          text,
    context_md     text        NOT NULL DEFAULT '',
    birthday_month smallint CHECK (birthday_month BETWEEN 1 AND 12),
    birthday_day   smallint CHECK (birthday_day BETWEEN 1 AND 31),
    birthday_year  smallint CHECK (birthday_year BETWEEN 1900 AND 2200),
    last_contact_on date,
    archived_at    timestamptz,
    version        integer     NOT NULL DEFAULT 1,
    created_at     timestamptz NOT NULL DEFAULT now(),
    updated_at     timestamptz NOT NULL DEFAULT now(),
    UNIQUE (user_id, id),
    FOREIGN KEY (user_id, area_id) REFERENCES areas (user_id, id) ON DELETE SET NULL (area_id),
    CHECK ((birthday_month IS NULL) = (birthday_day IS NULL)),
    CHECK (birthday_year IS NULL OR birthday_month IS NOT NULL)
);
CREATE INDEX people_user_name ON people (user_id, lower(immutable_unaccent(name)));
CREATE INDEX people_aliases ON people USING gin (aliases);
CREATE TRIGGER people_touch BEFORE UPDATE ON people FOR EACH ROW EXECUTE FUNCTION touch_updated_at();

CREATE TABLE person_notes (
    id         uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id    uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    person_id  uuid        NOT NULL,
    on_date    date        NOT NULL,
    text       text        NOT NULL CHECK (length(trim(text)) > 0),
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (user_id, id),
    FOREIGN KEY (user_id, person_id) REFERENCES people (user_id, id) ON DELETE CASCADE
);

CREATE TABLE interactions (
    id         uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id    uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    person_id  uuid        NOT NULL,
    on_date    date        NOT NULL,
    topic      text        NOT NULL DEFAULT '',
    source     text        NOT NULL DEFAULT 'manual' CHECK (source IN ('manual', 'calendar', 'gmail', 'bot', 'import')),
    source_ref text,
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (user_id, id),
    UNIQUE (user_id, person_id, source, source_ref),
    FOREIGN KEY (user_id, person_id) REFERENCES people (user_id, id) ON DELETE CASCADE
);
CREATE INDEX interactions_user_date ON interactions (user_id, on_date);

-- Personas detectadas (agenda, correo, dictado) que el usuario decide crear o descartar.
CREATE TABLE person_suggestions (
    id         uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id    uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    name       text        NOT NULL,
    email      text,
    source     text        NOT NULL CHECK (source IN ('calendar', 'gmail', 'dictation', 'import')),
    source_ref text        NOT NULL,
    on_date    date,
    context    text        NOT NULL DEFAULT '',
    status     text        NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'accepted', 'dismissed')),
    person_id  uuid,
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (user_id, id),
    UNIQUE (user_id, source, source_ref),
    FOREIGN KEY (user_id, person_id) REFERENCES people (user_id, id) ON DELETE SET NULL (person_id)
);

-- Reglas de pendientes recurrentes (día N, último o penúltimo del mes).
CREATE TABLE task_recurrence_rules (
    id         uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id    uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    title      text        NOT NULL CHECK (length(trim(title)) > 0),
    area_id    uuid,
    project_id uuid,
    day_kind   text        NOT NULL CHECK (day_kind IN ('day', 'last', 'penultimate')),
    day        smallint CHECK (day BETWEEN 1 AND 31),
    active     boolean     NOT NULL DEFAULT true,
    starts_on  date        NOT NULL,
    version    integer     NOT NULL DEFAULT 1,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (user_id, id),
    FOREIGN KEY (user_id, area_id) REFERENCES areas (user_id, id) ON DELETE SET NULL (area_id),
    FOREIGN KEY (user_id, project_id) REFERENCES projects (user_id, id) ON DELETE SET NULL (project_id),
    CHECK ((day_kind = 'day') = (day IS NOT NULL))
);
CREATE TRIGGER task_recurrence_rules_touch BEFORE UPDATE ON task_recurrence_rules FOR EACH ROW EXECUTE FUNCTION touch_updated_at();

CREATE TABLE tasks (
    id                       uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id                  uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    title                    text        NOT NULL CHECK (length(trim(title)) > 0),
    notes_md                 text        NOT NULL DEFAULT '',
    area_id                  uuid,
    project_id               uuid,
    status                   text        NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'done', 'cancelled')),
    priority                 smallint CHECK (priority BETWEEN 1 AND 4),
    due_on                   date,
    planned_on               date,
    done_at                  timestamptz,
    recurrence_rule_id       uuid,
    recurrence_period        text,
    class_note_id            uuid,
    external_source          text CHECK (external_source IN ('clickup')),
    external_id              text,
    external_url             text,
    external_status          text,
    external_status_category text CHECK (external_status_category IN ('open', 'need_info', 'closed')),
    external_priority        text,
    ease_hint                text,
    external_synced_at       timestamptz,
    archived_at              timestamptz,
    version                  integer     NOT NULL DEFAULT 1,
    created_at               timestamptz NOT NULL DEFAULT now(),
    updated_at               timestamptz NOT NULL DEFAULT now(),
    UNIQUE (user_id, id),
    UNIQUE (user_id, recurrence_rule_id, recurrence_period),
    UNIQUE (user_id, external_source, external_id),
    FOREIGN KEY (user_id, area_id) REFERENCES areas (user_id, id) ON DELETE SET NULL (area_id),
    FOREIGN KEY (user_id, project_id) REFERENCES projects (user_id, id) ON DELETE SET NULL (project_id),
    FOREIGN KEY (user_id, recurrence_rule_id) REFERENCES task_recurrence_rules (user_id, id) ON DELETE SET NULL (recurrence_rule_id),
    CHECK ((external_source IS NULL) = (external_id IS NULL))
);
CREATE INDEX tasks_user_open ON tasks (user_id, status, planned_on);
CREATE TRIGGER tasks_touch BEFORE UPDATE ON tasks FOR EACH ROW EXECUTE FUNCTION touch_updated_at();

CREATE TABLE task_people (
    user_id   uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    task_id   uuid NOT NULL,
    person_id uuid NOT NULL,
    PRIMARY KEY (task_id, person_id),
    FOREIGN KEY (user_id, task_id) REFERENCES tasks (user_id, id) ON DELETE CASCADE,
    FOREIGN KEY (user_id, person_id) REFERENCES people (user_id, id) ON DELETE CASCADE
);

-- Historial de la tarea: el arrastre no crea tareas nuevas, registra un evento.
CREATE TABLE task_events (
    id        bigserial PRIMARY KEY,
    user_id   uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    task_id   uuid        NOT NULL,
    at        timestamptz NOT NULL DEFAULT now(),
    kind      text        NOT NULL CHECK (kind IN ('created', 'carried_over', 'completed', 'reopened', 'edited', 'cancelled', 'external_sync')),
    from_date date,
    to_date   date,
    data      jsonb       NOT NULL DEFAULT '{}',
    FOREIGN KEY (user_id, task_id) REFERENCES tasks (user_id, id) ON DELETE CASCADE
);
CREATE INDEX task_events_task ON task_events (task_id, at);

CREATE TABLE inbox_items (
    id              uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id         uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    text            text        NOT NULL CHECK (length(trim(text)) > 0),
    source          text        NOT NULL DEFAULT 'app' CHECK (source IN ('app', 'telegram', 'import')),
    status          text        NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'processed', 'discarded', 'archived')),
    captured_at     timestamptz NOT NULL DEFAULT now(),
    proposal        jsonb,
    proposal_source text CHECK (proposal_source IN ('manual', 'llm', 'rule')),
    processed_at    timestamptz,
    result_type     text,
    result_id       uuid,
    client_id       text,
    version         integer     NOT NULL DEFAULT 1,
    updated_at      timestamptz NOT NULL DEFAULT now(),
    UNIQUE (user_id, id),
    UNIQUE (user_id, client_id)
);
CREATE INDEX inbox_user_status ON inbox_items (user_id, status, captured_at);
CREATE TRIGGER inbox_items_touch BEFORE UPDATE ON inbox_items FOR EACH ROW EXECUTE FUNCTION touch_updated_at();

CREATE TABLE weekly_reviews (
    id            uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id       uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    iso_week      text        NOT NULL CHECK (iso_week ~ '^\d{4}-W\d{2}$'),
    week_start    date        NOT NULL,
    peace         smallint CHECK (peace BETWEEN 1 AND 5),
    what_happened text        NOT NULL DEFAULT '',
    what_didnt    text        NOT NULL DEFAULT '',
    content_md    text        NOT NULL DEFAULT '',
    habits_md     text        NOT NULL DEFAULT '',
    goals_md      text        NOT NULL DEFAULT '',
    went_well     text        NOT NULL DEFAULT '',
    went_badly    text        NOT NULL DEFAULT '',
    areas_md      text        NOT NULL DEFAULT '',
    people_md     text        NOT NULL DEFAULT '',
    priorities    jsonb       NOT NULL DEFAULT '[]',
    summary       jsonb,
    evidence      jsonb       NOT NULL DEFAULT '[]',
    completed_at  timestamptz,
    version       integer     NOT NULL DEFAULT 1,
    created_at    timestamptz NOT NULL DEFAULT now(),
    updated_at    timestamptz NOT NULL DEFAULT now(),
    UNIQUE (user_id, id),
    UNIQUE (user_id, iso_week),
    CHECK (jsonb_typeof(priorities) = 'array' AND jsonb_array_length(priorities) <= 3)
);
CREATE TRIGGER weekly_reviews_touch BEFORE UPDATE ON weekly_reviews FOR EACH ROW EXECUTE FUNCTION touch_updated_at();
