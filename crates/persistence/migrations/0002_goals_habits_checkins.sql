-- Objetivos, progreso cuantitativo, hábitos versionados, check-ins diarios y agenda.

CREATE TABLE goals (
    id               uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id          uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    kind             text        NOT NULL CHECK (kind IN ('month', 'week')),
    period_key       text        NOT NULL CHECK (period_key ~ '^\d{4}-(\d{2}|W\d{2})$'),
    starts_on        date        NOT NULL,
    ends_on          date        NOT NULL CHECK (ends_on >= starts_on),
    title            text        NOT NULL CHECK (length(trim(title)) > 0),
    area_id          uuid,
    motivation       text        NOT NULL DEFAULT '',
    success_criteria text        NOT NULL DEFAULT '',
    status           text        NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'achieved', 'not_achieved')),
    parent_goal_id   uuid CHECK (parent_goal_id IS NULL OR kind = 'week'),
    progress_type    text        NOT NULL DEFAULT 'none' CHECK (progress_type IN ('none', 'cumulative', 'current_value', 'milestone')),
    unit             text,
    baseline         numeric(14, 3),
    target           numeric(14, 3),
    direction        text CHECK (direction IN ('increase', 'decrease')),
    progress_source  text        NOT NULL DEFAULT 'manual' CHECK (progress_source IN ('manual', 'readings', 'habit', 'weight', 'debt', 'finance')),
    source_ref       uuid,
    closing          jsonb,
    closed_at        timestamptz,
    position         integer     NOT NULL DEFAULT 0,
    archived_at      timestamptz,
    version          integer     NOT NULL DEFAULT 1,
    created_at       timestamptz NOT NULL DEFAULT now(),
    updated_at       timestamptz NOT NULL DEFAULT now(),
    UNIQUE (user_id, id),
    FOREIGN KEY (user_id, area_id) REFERENCES areas (user_id, id) ON DELETE SET NULL (area_id),
    FOREIGN KEY (user_id, parent_goal_id) REFERENCES goals (user_id, id) ON DELETE SET NULL (parent_goal_id),
    CHECK (progress_type = 'none' OR progress_type = 'milestone' OR (unit IS NOT NULL AND target IS NOT NULL))
);
CREATE INDEX goals_period ON goals (user_id, kind, period_key);
CREATE TRIGGER goals_touch BEFORE UPDATE ON goals FOR EACH ROW EXECUTE FUNCTION touch_updated_at();

-- Eventos de progreso. Una fuente (lectura, deuda, transacción…) aporta como máximo una vez a cada objetivo.
CREATE TABLE goal_progress_events (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id     uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    goal_id     uuid        NOT NULL,
    occurred_on date        NOT NULL,
    amount      numeric(14, 3),
    value       numeric(14, 3),
    completed   boolean,
    source_type text        NOT NULL DEFAULT 'manual' CHECK (source_type IN ('manual', 'reading', 'habit_entry', 'debt', 'transaction', 'weight', 'content')),
    source_id   uuid,
    note        text        NOT NULL DEFAULT '',
    created_at  timestamptz NOT NULL DEFAULT now(),
    UNIQUE (user_id, id),
    UNIQUE (goal_id, source_type, source_id),
    FOREIGN KEY (user_id, goal_id) REFERENCES goals (user_id, id) ON DELETE CASCADE,
    CHECK (num_nonnulls(amount, value, completed) = 1)
);
CREATE INDEX goal_progress_goal ON goal_progress_events (goal_id, occurred_on);

CREATE TABLE habits (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id     uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    name        text        NOT NULL CHECK (length(trim(name)) > 0),
    kind        text        NOT NULL CHECK (kind IN ('fixed', 'goal', 'custom')),
    fixed_key   text CHECK (fixed_key IN ('devotional', 'exercise', 'reading')),
    goal_id     uuid,
    area_id     uuid,
    unit        text,
    description text        NOT NULL DEFAULT '',
    position    integer     NOT NULL DEFAULT 0,
    created_on  date        NOT NULL,
    archived_at timestamptz,
    version     integer     NOT NULL DEFAULT 1,
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now(),
    UNIQUE (user_id, id),
    UNIQUE (user_id, fixed_key),
    FOREIGN KEY (user_id, goal_id) REFERENCES goals (user_id, id) ON DELETE SET NULL (goal_id),
    FOREIGN KEY (user_id, area_id) REFERENCES areas (user_id, id) ON DELETE SET NULL (area_id),
    CHECK ((kind = 'fixed') = (fixed_key IS NOT NULL))
);
CREATE TRIGGER habits_touch BEFORE UPDATE ON habits FOR EACH ROW EXECUTE FUNCTION touch_updated_at();

-- Versiones de frecuencia/meta con vigencia [valid_from, valid_to] (valid_to inclusive, NULL = abierta).
-- weekdays: bitmask ISO (lunes = 1, martes = 2, …, domingo = 64).
CREATE TABLE habit_versions (
    id              uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id         uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    habit_id        uuid        NOT NULL,
    valid_from      date        NOT NULL,
    valid_to        date CHECK (valid_to IS NULL OR valid_to >= valid_from),
    frequency       text        NOT NULL CHECK (frequency IN ('daily', 'weekdays', 'times_per_week')),
    weekdays        smallint CHECK (weekdays BETWEEN 1 AND 127),
    times_per_week  smallint CHECK (times_per_week BETWEEN 1 AND 7),
    target_quantity numeric(10, 2) CHECK (target_quantity > 0),
    created_at      timestamptz NOT NULL DEFAULT now(),
    UNIQUE (user_id, id),
    UNIQUE (habit_id, valid_from),
    FOREIGN KEY (user_id, habit_id) REFERENCES habits (user_id, id) ON DELETE CASCADE,
    CHECK ((frequency = 'weekdays') = (weekdays IS NOT NULL)),
    CHECK ((frequency = 'times_per_week') = (times_per_week IS NOT NULL))
);

CREATE TABLE habit_pauses (
    id         uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id    uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    habit_id   uuid        NOT NULL,
    starts_on  date        NOT NULL,
    ends_on    date CHECK (ends_on IS NULL OR ends_on >= starts_on),
    reason     text        NOT NULL DEFAULT '',
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (user_id, id),
    FOREIGN KEY (user_id, habit_id) REFERENCES habits (user_id, id) ON DELETE CASCADE
);

-- Una entrada por hábito y fecha local. Sin fila = sin registrar (≠ no cumplido).
CREATE TABLE habit_entries (
    user_id    uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    habit_id   uuid        NOT NULL,
    on_date    date        NOT NULL,
    status     text        NOT NULL CHECK (status IN ('done', 'not_done', 'not_applicable')),
    quantity   numeric(10, 2) CHECK (quantity >= 0),
    reason     text,
    source     text        NOT NULL DEFAULT 'manual' CHECK (source IN ('manual', 'checkin', 'devotional', 'weight', 'expense', 'reflection', 'import', 'bot')),
    version    integer     NOT NULL DEFAULT 1,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (habit_id, on_date),
    FOREIGN KEY (user_id, habit_id) REFERENCES habits (user_id, id) ON DELETE CASCADE,
    CHECK (status <> 'not_applicable' OR length(trim(coalesce(reason, ''))) > 0)
);
CREATE INDEX habit_entries_user_date ON habit_entries (user_id, on_date);
CREATE TRIGGER habit_entries_touch BEFORE UPDATE ON habit_entries FOR EACH ROW EXECUTE FUNCTION touch_updated_at();

-- Vínculos explícitos y configurables (nunca por texto): un disparador → un hábito.
CREATE TABLE habit_links (
    user_id  uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    trigger  text NOT NULL CHECK (trigger IN ('weight_logged', 'expense_logged', 'positive_reflection', 'devotional_content')),
    habit_id uuid NOT NULL,
    PRIMARY KEY (user_id, trigger),
    FOREIGN KEY (user_id, habit_id) REFERENCES habits (user_id, id) ON DELETE CASCADE
);

CREATE TABLE daily_checkins (
    id                  uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id             uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    on_date             date        NOT NULL,
    weight_kg           numeric(5, 2) CHECK (weight_kg > 0 AND weight_kg < 500),
    energy              smallint CHECK (energy BETWEEN 1 AND 5),
    planned_spend_text  text,
    planned_spend_cents bigint CHECK (planned_spend_cents >= 0),
    planned_spend_none  boolean     NOT NULL DEFAULT false,
    notes_md            text        NOT NULL DEFAULT '',
    morning_done_at     timestamptz,
    evening_done_at     timestamptz,
    version             integer     NOT NULL DEFAULT 1,
    created_at          timestamptz NOT NULL DEFAULT now(),
    updated_at          timestamptz NOT NULL DEFAULT now(),
    UNIQUE (user_id, id),
    UNIQUE (user_id, on_date)
);
CREATE TRIGGER daily_checkins_touch BEFORE UPDATE ON daily_checkins FOR EACH ROW EXECUTE FUNCTION touch_updated_at();

CREATE TABLE daily_log_entries (
    id         uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id    uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    on_date    date        NOT NULL,
    text       text        NOT NULL CHECK (length(trim(text)) > 0),
    position   integer     NOT NULL DEFAULT 0,
    source     text        NOT NULL DEFAULT 'manual' CHECK (source IN ('manual', 'bot', 'llm', 'import')),
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (user_id, id)
);
CREATE INDEX daily_log_user_date ON daily_log_entries (user_id, on_date);

CREATE TABLE reflection_items (
    id         uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id    uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    on_date    date        NOT NULL,
    polarity   text        NOT NULL CHECK (polarity IN ('good', 'bad')),
    text       text        NOT NULL CHECK (length(trim(text)) > 0),
    position   integer     NOT NULL DEFAULT 0,
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (user_id, id)
);
CREATE INDEX reflection_user_date ON reflection_items (user_id, on_date);

CREATE TABLE calendar_events (
    id                uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id           uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    source            text        NOT NULL DEFAULT 'manual' CHECK (source IN ('google', 'manual')),
    calendar_id       text        NOT NULL DEFAULT 'manual',
    provider_event_id text        NOT NULL DEFAULT gen_random_uuid()::text,
    on_date           date        NOT NULL,
    starts_at         timestamptz,
    ends_at           timestamptz,
    all_day           boolean     NOT NULL DEFAULT false,
    title             text        NOT NULL,
    location          text,
    attendees         jsonb       NOT NULL DEFAULT '[]',
    status            text        NOT NULL DEFAULT 'confirmed',
    html_link         text,
    synced_at         timestamptz,
    created_at        timestamptz NOT NULL DEFAULT now(),
    updated_at        timestamptz NOT NULL DEFAULT now(),
    UNIQUE (user_id, id),
    UNIQUE (user_id, source, calendar_id, provider_event_id)
);
CREATE INDEX calendar_events_user_date ON calendar_events (user_id, on_date);
CREATE TRIGGER calendar_events_touch BEFORE UPDATE ON calendar_events FOR EACH ROW EXECUTE FUNCTION touch_updated_at();
