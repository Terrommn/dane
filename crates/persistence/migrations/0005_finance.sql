-- Finanzas: dinero en centavos BIGINT (D-009), relaciones por ID (no por texto, C4).

CREATE TABLE fin_categories (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id     uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    name        text        NOT NULL CHECK (length(trim(name)) > 0),
    kind        text        NOT NULL CHECK (kind IN ('expense', 'income')),
    -- consumption: consumo · debt_payment: pago de deuda · investment · payroll (nómina, consumo de negocio)
    -- earned: ingreso ganado · financing: préstamo recibido · other_income
    role        text        NOT NULL CHECK (role IN ('consumption', 'debt_payment', 'investment', 'payroll', 'earned', 'financing', 'other_income')),
    color       text,
    position    integer     NOT NULL DEFAULT 0,
    archived_at timestamptz,
    version     integer     NOT NULL DEFAULT 1,
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now(),
    UNIQUE (user_id, id),
    UNIQUE (user_id, kind, name),
    CHECK ((kind = 'expense') = (role IN ('consumption', 'debt_payment', 'investment', 'payroll')))
);
CREATE TRIGGER fin_categories_touch BEFORE UPDATE ON fin_categories FOR EACH ROW EXECUTE FUNCTION touch_updated_at();

CREATE TABLE fin_accounts (
    id                 uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id            uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    name               text        NOT NULL CHECK (length(trim(name)) > 0),
    bank               text        NOT NULL DEFAULT '',
    kind               text        NOT NULL CHECK (kind IN ('debit', 'cash', 'savings', 'credit_card')),
    currency           char(3)     NOT NULL DEFAULT 'MXN',
    credit_limit_cents bigint CHECK (credit_limit_cents >= 0),
    cut_day            smallint CHECK (cut_day BETWEEN 1 AND 31),
    pay_day            smallint CHECK (pay_day BETWEEN 1 AND 31),
    status             text        NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'to_cancel', 'cancelled', 'closed')),
    is_default_debit   boolean     NOT NULL DEFAULT false,
    notes_md           text        NOT NULL DEFAULT '',
    archived_at        timestamptz,
    version            integer     NOT NULL DEFAULT 1,
    created_at         timestamptz NOT NULL DEFAULT now(),
    updated_at         timestamptz NOT NULL DEFAULT now(),
    UNIQUE (user_id, id),
    CHECK (kind = 'credit_card' OR (credit_limit_cents IS NULL AND cut_day IS NULL AND pay_day IS NULL)),
    CHECK (NOT is_default_debit OR kind <> 'credit_card')
);
CREATE UNIQUE INDEX fin_accounts_one_default_debit ON fin_accounts (user_id) WHERE is_default_debit;
CREATE TRIGGER fin_accounts_touch BEFORE UPDATE ON fin_accounts FOR EACH ROW EXECUTE FUNCTION touch_updated_at();

-- Saldo observado. Débito: saldo. Tarjeta: crédito disponible. Ver D-020 para la semántica.
CREATE TABLE balance_snapshots (
    id            uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id       uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    account_id    uuid        NOT NULL,
    observed_at   timestamptz NOT NULL,
    observed_on   date        NOT NULL,
    balance_cents bigint      NOT NULL,
    semantics     text        NOT NULL DEFAULT 'instant' CHECK (semantics IN ('instant', 'legacy_day_start_inclusive', 'legacy_after_day')),
    note          text        NOT NULL DEFAULT '',
    created_at    timestamptz NOT NULL DEFAULT now(),
    UNIQUE (user_id, id),
    FOREIGN KEY (user_id, account_id) REFERENCES fin_accounts (user_id, id) ON DELETE CASCADE
);
CREATE INDEX balance_snapshots_account ON balance_snapshots (account_id, observed_at DESC);

CREATE TABLE debts (
    id              uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id         uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    name            text        NOT NULL CHECK (length(trim(name)) > 0),
    kind            text        NOT NULL CHECK (kind IN ('credit_card', 'loan', 'family', 'other')),
    account_id      uuid,
    person_id       uuid,
    principal_cents bigint CHECK (principal_cents >= 0),
    currency        char(3)     NOT NULL DEFAULT 'MXN',
    opened_on       date        NOT NULL,
    status          text        NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'paid', 'cancelled')),
    paid_on         date,
    goal_id         uuid,
    notes_md        text        NOT NULL DEFAULT '',
    version         integer     NOT NULL DEFAULT 1,
    created_at      timestamptz NOT NULL DEFAULT now(),
    updated_at      timestamptz NOT NULL DEFAULT now(),
    UNIQUE (user_id, id),
    FOREIGN KEY (user_id, account_id) REFERENCES fin_accounts (user_id, id) ON DELETE SET NULL (account_id),
    FOREIGN KEY (user_id, person_id) REFERENCES people (user_id, id) ON DELETE SET NULL (person_id),
    FOREIGN KEY (user_id, goal_id) REFERENCES goals (user_id, id) ON DELETE SET NULL (goal_id),
    CHECK (kind = 'credit_card' OR principal_cents IS NOT NULL)
);
CREATE TRIGGER debts_touch BEFORE UPDATE ON debts FOR EACH ROW EXECUTE FUNCTION touch_updated_at();

CREATE TABLE fin_recurring_rules (
    id           uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id      uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    kind         text        NOT NULL CHECK (kind IN ('subscription', 'fixed_income')),
    name         text        NOT NULL CHECK (length(trim(name)) > 0),
    amount_cents bigint      NOT NULL CHECK (amount_cents > 0),
    currency     char(3)     NOT NULL DEFAULT 'MXN',
    day_kind     text        NOT NULL CHECK (day_kind IN ('day', 'last', 'penultimate')),
    day          smallint CHECK (day BETWEEN 1 AND 31),
    category_id  uuid,
    account_id   uuid,
    area_id      uuid,
    project_id   uuid,
    status       text        NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'cancelled', 'ended')),
    starts_on    date        NOT NULL,
    ends_on      date,
    notes_md     text        NOT NULL DEFAULT '',
    version      integer     NOT NULL DEFAULT 1,
    created_at   timestamptz NOT NULL DEFAULT now(),
    updated_at   timestamptz NOT NULL DEFAULT now(),
    UNIQUE (user_id, id),
    FOREIGN KEY (user_id, category_id) REFERENCES fin_categories (user_id, id) ON DELETE SET NULL (category_id),
    FOREIGN KEY (user_id, account_id) REFERENCES fin_accounts (user_id, id) ON DELETE SET NULL (account_id),
    FOREIGN KEY (user_id, area_id) REFERENCES areas (user_id, id) ON DELETE SET NULL (area_id),
    FOREIGN KEY (user_id, project_id) REFERENCES projects (user_id, id) ON DELETE SET NULL (project_id),
    CHECK ((day_kind = 'day') = (day IS NOT NULL))
);
CREATE TRIGGER fin_recurring_rules_touch BEFORE UPDATE ON fin_recurring_rules FOR EACH ROW EXECUTE FUNCTION touch_updated_at();

CREATE TABLE budget_months (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id     uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    month       text        NOT NULL CHECK (month ~ '^\d{4}-\d{2}$'),
    copied_from text,
    version     integer     NOT NULL DEFAULT 1,
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now(),
    UNIQUE (user_id, id),
    UNIQUE (user_id, month)
);
CREATE TRIGGER budget_months_touch BEFORE UPDATE ON budget_months FOR EACH ROW EXECUTE FUNCTION touch_updated_at();

CREATE TABLE budget_lines (
    id              uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id         uuid   NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    budget_month_id uuid   NOT NULL,
    category_id     uuid   NOT NULL,
    amount_cents    bigint NOT NULL CHECK (amount_cents >= 0),
    UNIQUE (user_id, id),
    UNIQUE (budget_month_id, category_id),
    FOREIGN KEY (user_id, budget_month_id) REFERENCES budget_months (user_id, id) ON DELETE CASCADE,
    FOREIGN KEY (user_id, category_id) REFERENCES fin_categories (user_id, id) ON DELETE CASCADE
);

-- Rubros de proyecto (ej. Negocio: Servidores, WhatsApp, Ads). La línea de su categoría = suma de rubros.
CREATE TABLE budget_items (
    id              uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id         uuid   NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    budget_month_id uuid   NOT NULL,
    project_id      uuid   NOT NULL,
    category_id     uuid   NOT NULL,
    name            text   NOT NULL CHECK (length(trim(name)) > 0),
    amount_cents    bigint NOT NULL CHECK (amount_cents >= 0),
    keywords        text[] NOT NULL DEFAULT '{}',
    position        integer NOT NULL DEFAULT 0,
    UNIQUE (user_id, id),
    UNIQUE (budget_month_id, project_id, name),
    FOREIGN KEY (user_id, budget_month_id) REFERENCES budget_months (user_id, id) ON DELETE CASCADE,
    FOREIGN KEY (user_id, project_id) REFERENCES projects (user_id, id) ON DELETE CASCADE,
    FOREIGN KEY (user_id, category_id) REFERENCES fin_categories (user_id, id) ON DELETE CASCADE
);

CREATE TABLE transactions (
    id                    uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id               uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    kind                  text        NOT NULL CHECK (kind IN ('expense', 'income', 'transfer', 'card_payment', 'debt_payment', 'loan_received', 'adjustment')),
    status                text        NOT NULL DEFAULT 'confirmed' CHECK (status IN ('confirmed', 'pending_reconciliation', 'void')),
    origin                text        NOT NULL DEFAULT 'manual' CHECK (origin IN ('manual', 'recurring', 'import', 'llm', 'bot')),
    occurred_on           date        NOT NULL,
    occurred_at           timestamptz,
    concept               text        NOT NULL CHECK (length(trim(concept)) > 0),
    amount_cents          bigint      NOT NULL CHECK (amount_cents > 0),
    currency              char(3)     NOT NULL DEFAULT 'MXN',
    original_amount_cents bigint CHECK (original_amount_cents > 0),
    original_currency     char(3),
    fx_rate               numeric(18, 8) CHECK (fx_rate > 0),
    fx_date               date,
    fx_source             text,
    category_id           uuid,
    account_id            uuid,
    counter_account_id    uuid,
    debt_id               uuid,
    person_id             uuid,
    financed_by_person_id uuid,
    project_id            uuid,
    area_id               uuid,
    budget_item_id        uuid,
    recurring_rule_id     uuid,
    recurrence_period     text,
    idempotency_key       text,
    adjustment_sign       smallint CHECK (adjustment_sign IN (-1, 1)),
    notes_md              text        NOT NULL DEFAULT '',
    confirmed_at          timestamptz,
    version               integer     NOT NULL DEFAULT 1,
    created_at            timestamptz NOT NULL DEFAULT now(),
    updated_at            timestamptz NOT NULL DEFAULT now(),
    UNIQUE (user_id, id),
    UNIQUE (user_id, idempotency_key),
    UNIQUE (recurring_rule_id, recurrence_period),
    FOREIGN KEY (user_id, category_id) REFERENCES fin_categories (user_id, id) ON DELETE SET NULL (category_id),
    FOREIGN KEY (user_id, account_id) REFERENCES fin_accounts (user_id, id) ON DELETE RESTRICT,
    FOREIGN KEY (user_id, counter_account_id) REFERENCES fin_accounts (user_id, id) ON DELETE RESTRICT,
    FOREIGN KEY (user_id, debt_id) REFERENCES debts (user_id, id) ON DELETE SET NULL (debt_id),
    FOREIGN KEY (user_id, person_id) REFERENCES people (user_id, id) ON DELETE SET NULL (person_id),
    FOREIGN KEY (user_id, financed_by_person_id) REFERENCES people (user_id, id) ON DELETE SET NULL (financed_by_person_id),
    FOREIGN KEY (user_id, project_id) REFERENCES projects (user_id, id) ON DELETE SET NULL (project_id),
    FOREIGN KEY (user_id, area_id) REFERENCES areas (user_id, id) ON DELETE SET NULL (area_id),
    FOREIGN KEY (user_id, budget_item_id) REFERENCES budget_items (user_id, id) ON DELETE SET NULL (budget_item_id),
    FOREIGN KEY (user_id, recurring_rule_id) REFERENCES fin_recurring_rules (user_id, id) ON DELETE SET NULL (recurring_rule_id),
    -- Moneda distinta: monto original y conversión explícita completos o ninguno (C8).
    CHECK ((original_currency IS NULL) = (original_amount_cents IS NULL)),
    CHECK (original_currency IS NULL OR (fx_rate IS NOT NULL AND fx_date IS NOT NULL AND length(trim(coalesce(fx_source, ''))) > 0)),
    CHECK ((recurring_rule_id IS NULL) = (recurrence_period IS NULL) OR recurring_rule_id IS NULL),
    CHECK (kind NOT IN ('transfer', 'card_payment') OR (account_id IS NOT NULL AND counter_account_id IS NOT NULL AND account_id <> counter_account_id)),
    CHECK (kind <> 'adjustment' OR (adjustment_sign IS NOT NULL AND account_id IS NOT NULL)),
    CHECK (kind <> 'debt_payment' OR debt_id IS NOT NULL OR counter_account_id IS NOT NULL)
);
CREATE INDEX transactions_user_date ON transactions (user_id, occurred_on);
CREATE INDEX transactions_account ON transactions (account_id, occurred_on);
CREATE INDEX transactions_counter ON transactions (counter_account_id, occurred_on);
CREATE TRIGGER transactions_touch BEFORE UPDATE ON transactions FOR EACH ROW EXECUTE FUNCTION touch_updated_at();
CREATE TRIGGER transactions_search AFTER INSERT OR UPDATE OR DELETE ON transactions FOR EACH ROW EXECUTE FUNCTION search_index_trigger('transaction', 'concept', 'notes_md,kind', 'occurred_on', 'status');

CREATE TABLE credit_plans (
    user_id                uuid PRIMARY KEY REFERENCES users (id) ON DELETE CASCADE,
    goal_text              text        NOT NULL DEFAULT '',
    target_date            date,
    steps                  jsonb       NOT NULL DEFAULT '[]' CHECK (jsonb_typeof(steps) = 'array'),
    rules                  jsonb       NOT NULL DEFAULT '[]' CHECK (jsonb_typeof(rules) = 'array'),
    max_usage_pct          smallint    NOT NULL DEFAULT 30 CHECK (max_usage_pct BETWEEN 1 AND 100),
    excluded_account_ids   uuid[]      NOT NULL DEFAULT '{}',
    forbid_cards_with_debt boolean     NOT NULL DEFAULT true,
    version                integer     NOT NULL DEFAULT 1,
    updated_at             timestamptz NOT NULL DEFAULT now()
);
CREATE TRIGGER credit_plans_touch BEFORE UPDATE ON credit_plans FOR EACH ROW EXECUTE FUNCTION touch_updated_at();
