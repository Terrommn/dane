-- Notas, enlaces, favoritos, búsqueda, dibujos, devocionales, lecturas, contenido, Swipe, escuela y desarrollo.

CREATE TABLE notes (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id     uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    kind        text        NOT NULL DEFAULT 'note' CHECK (kind IN ('note', 'runbook', 'til', 'snippet')),
    title       text        NOT NULL CHECK (length(trim(title)) > 0),
    body_md     text        NOT NULL DEFAULT '',
    language    text,
    aliases     text[]      NOT NULL DEFAULT '{}',
    tags        text[]      NOT NULL DEFAULT '{}',
    area_id     uuid,
    project_id  uuid,
    metadata    jsonb       NOT NULL DEFAULT '{}',
    archived_at timestamptz,
    version     integer     NOT NULL DEFAULT 1,
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now(),
    UNIQUE (user_id, id),
    FOREIGN KEY (user_id, area_id) REFERENCES areas (user_id, id) ON DELETE SET NULL (area_id),
    FOREIGN KEY (user_id, project_id) REFERENCES projects (user_id, id) ON DELETE SET NULL (project_id)
);
CREATE INDEX notes_tags ON notes USING gin (tags);
CREATE TRIGGER notes_touch BEFORE UPDATE ON notes FOR EACH ROW EXECUTE FUNCTION touch_updated_at();

-- Enlaces entre entidades (wikilinks/backlinks). Polimórficos: el server valida que ambos extremos sean del usuario.
CREATE TABLE entity_links (
    user_id    uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    src_type   text        NOT NULL,
    src_id     uuid        NOT NULL,
    dst_type   text        NOT NULL,
    dst_id     uuid        NOT NULL,
    relation   text        NOT NULL DEFAULT 'related',
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, src_type, src_id, dst_type, dst_id, relation),
    CHECK (NOT (src_type = dst_type AND src_id = dst_id))
);
CREATE INDEX entity_links_dst ON entity_links (user_id, dst_type, dst_id);

CREATE TABLE favorites (
    user_id     uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    entity_type text        NOT NULL,
    entity_id   uuid        NOT NULL,
    created_at  timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, entity_type, entity_id)
);

-- Índice de búsqueda global alimentado por triggers (D-015).
CREATE TABLE search_index (
    user_id     uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    entity_type text        NOT NULL,
    entity_id   uuid        NOT NULL,
    title       text        NOT NULL DEFAULT '',
    body        text        NOT NULL DEFAULT '',
    tags        text[]      NOT NULL DEFAULT '{}',
    area_id     uuid,
    on_date     date,
    status      text,
    archived    boolean     NOT NULL DEFAULT false,
    updated_at  timestamptz NOT NULL DEFAULT now(),
    doc         tsvector GENERATED ALWAYS AS (
        setweight(to_tsvector('simple', immutable_unaccent(lower(title))), 'A') ||
        setweight(to_tsvector('simple', immutable_unaccent(lower(body))), 'B')
    ) STORED,
    PRIMARY KEY (entity_type, entity_id)
);
CREATE INDEX search_index_doc ON search_index USING gin (doc);
CREATE INDEX search_index_title_trgm ON search_index USING gin (immutable_unaccent(lower(title)) gin_trgm_ops);
CREATE INDEX search_index_user ON search_index (user_id, entity_type);

-- Trigger genérico: TG_ARGV = (entity_type, columna_titulo, columnas_cuerpo_csv, columna_fecha|'', columna_estado|'').
CREATE OR REPLACE FUNCTION search_index_trigger() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE
    r    jsonb;
    body text := '';
    c    text;
    tags text[] := '{}';
BEGIN
    IF TG_OP = 'DELETE' THEN
        DELETE FROM search_index WHERE entity_type = TG_ARGV[0] AND entity_id = OLD.id;
        RETURN OLD;
    END IF;
    r := to_jsonb(NEW);
    IF TG_ARGV[2] <> '' THEN
        FOREACH c IN ARRAY string_to_array(TG_ARGV[2], ',') LOOP
            IF jsonb_typeof(r -> c) = 'array' THEN
                body := body || ' ' || coalesce((SELECT string_agg(value::text, ' ') FROM jsonb_array_elements(r -> c)), '');
            ELSE
                body := body || ' ' || coalesce(r ->> c, '');
            END IF;
        END LOOP;
    END IF;
    IF jsonb_typeof(r -> 'tags') = 'array' THEN
        tags := ARRAY(SELECT jsonb_array_elements_text(r -> 'tags'));
    END IF;
    IF jsonb_typeof(r -> 'aliases') = 'array' THEN
        body := body || ' ' || array_to_string(ARRAY(SELECT jsonb_array_elements_text(r -> 'aliases')), ' ');
    END IF;
    INSERT INTO search_index (user_id, entity_type, entity_id, title, body, tags, area_id, on_date, status, archived, updated_at)
    VALUES (
        NEW.user_id, TG_ARGV[0], NEW.id,
        left(coalesce(r ->> TG_ARGV[1], ''), 500),
        left(body, 20000),
        tags,
        NULLIF(r ->> 'area_id', '')::uuid,
        CASE WHEN TG_ARGV[3] <> '' THEN NULLIF(r ->> TG_ARGV[3], '')::date END,
        CASE WHEN TG_ARGV[4] <> '' THEN r ->> TG_ARGV[4] END,
        (r ->> 'archived_at') IS NOT NULL,
        now()
    )
    ON CONFLICT (entity_type, entity_id) DO UPDATE SET
        title = EXCLUDED.title, body = EXCLUDED.body, tags = EXCLUDED.tags, area_id = EXCLUDED.area_id,
        on_date = EXCLUDED.on_date, status = EXCLUDED.status, archived = EXCLUDED.archived, updated_at = now();
    RETURN NEW;
END $$;

CREATE TABLE drawings (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id     uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    title       text        NOT NULL CHECK (length(trim(title)) > 0),
    scene       jsonb       NOT NULL DEFAULT '{}',
    note_id     uuid,
    archived_at timestamptz,
    version     integer     NOT NULL DEFAULT 1,
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now(),
    UNIQUE (user_id, id),
    FOREIGN KEY (user_id, note_id) REFERENCES notes (user_id, id) ON DELETE SET NULL (note_id),
    CHECK (pg_column_size(scene) < 5000000)
);
CREATE TRIGGER drawings_touch BEFORE UPDATE ON drawings FOR EACH ROW EXECUTE FUNCTION touch_updated_at();

CREATE TABLE devotionals (
    id           uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id      uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    on_date      date        NOT NULL,
    mode         text        NOT NULL DEFAULT 'devotional' CHECK (mode IN ('devotional', 'meditation')),
    passage      text        NOT NULL DEFAULT '',
    verse_actions jsonb      NOT NULL DEFAULT '[]' CHECK (jsonb_typeof(verse_actions) = 'array'),
    learned      text        NOT NULL DEFAULT '',
    teaching     text        NOT NULL DEFAULT '',
    prayer       text        NOT NULL DEFAULT '',
    song         text        NOT NULL DEFAULT '',
    extra        jsonb       NOT NULL DEFAULT '{}',
    version      integer     NOT NULL DEFAULT 1,
    created_at   timestamptz NOT NULL DEFAULT now(),
    updated_at   timestamptz NOT NULL DEFAULT now(),
    UNIQUE (user_id, id),
    UNIQUE (user_id, on_date)
);
CREATE TRIGGER devotionals_touch BEFORE UPDATE ON devotionals FOR EACH ROW EXECUTE FUNCTION touch_updated_at();

CREATE TABLE readings (
    id             uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id        uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    kind           text        NOT NULL DEFAULT 'book' CHECK (kind IN ('book', 'article')),
    title          text        NOT NULL CHECK (length(trim(title)) > 0),
    author         text        NOT NULL DEFAULT '',
    url            text,
    status         text        NOT NULL DEFAULT 'want' CHECK (status IN ('want', 'reading', 'finished', 'abandoned')),
    rating         smallint CHECK (rating BETWEEN 1 AND 5),
    started_on     date,
    finished_on    date,
    ideas_md       text        NOT NULL DEFAULT '',
    quotes_md      text        NOT NULL DEFAULT '',
    application_md text        NOT NULL DEFAULT '',
    archived_at    timestamptz,
    version        integer     NOT NULL DEFAULT 1,
    created_at     timestamptz NOT NULL DEFAULT now(),
    updated_at     timestamptz NOT NULL DEFAULT now(),
    UNIQUE (user_id, id),
    CHECK (status <> 'finished' OR finished_on IS NOT NULL)
);
CREATE TRIGGER readings_touch BEFORE UPDATE ON readings FOR EACH ROW EXECUTE FUNCTION touch_updated_at();

CREATE TABLE content_pieces (
    id             uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id        uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    title          text        NOT NULL CHECK (length(trim(title)) > 0),
    idea           text        NOT NULL DEFAULT '',
    status         text        NOT NULL DEFAULT 'idea' CHECK (status IN ('idea', 'script', 'recorded', 'edited', 'published')),
    format         text        NOT NULL DEFAULT 'reel',
    platforms      text[]      NOT NULL DEFAULT '{}',
    pillar         text,
    hooks          text[]      NOT NULL DEFAULT '{}' CHECK (cardinality(hooks) <= 10),
    chosen_hook    smallint CHECK (chosen_hook BETWEEN 0 AND 9),
    script_hook    text        NOT NULL DEFAULT '',
    script_body    text        NOT NULL DEFAULT '',
    script_cta     text        NOT NULL DEFAULT '',
    shot_list      jsonb       NOT NULL DEFAULT '[]' CHECK (jsonb_typeof(shot_list) = 'array'),
    edit_notes     text        NOT NULL DEFAULT '',
    result_worked  text        NOT NULL DEFAULT '',
    result_change  text        NOT NULL DEFAULT '',
    publish_on     date,
    published_on   date,
    raw_url        text,
    render_url     text,
    views          bigint CHECK (views >= 0),
    retention_pct  numeric(5, 2) CHECK (retention_pct BETWEEN 0 AND 100),
    area_id        uuid,
    project_id     uuid,
    archived_at    timestamptz,
    version        integer     NOT NULL DEFAULT 1,
    created_at     timestamptz NOT NULL DEFAULT now(),
    updated_at     timestamptz NOT NULL DEFAULT now(),
    UNIQUE (user_id, id),
    FOREIGN KEY (user_id, area_id) REFERENCES areas (user_id, id) ON DELETE SET NULL (area_id),
    FOREIGN KEY (user_id, project_id) REFERENCES projects (user_id, id) ON DELETE SET NULL (project_id),
    CHECK (chosen_hook IS NULL OR chosen_hook < cardinality(hooks))
);
CREATE TRIGGER content_pieces_touch BEFORE UPDATE ON content_pieces FOR EACH ROW EXECUTE FUNCTION touch_updated_at();

CREATE TABLE swipe_refs (
    id           uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id      uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    title        text        NOT NULL DEFAULT '',
    url          text        NOT NULL,
    creator      text,
    platform     text,
    why_it_works text        NOT NULL DEFAULT '',
    hook         text        NOT NULL DEFAULT '',
    tags         text[]      NOT NULL DEFAULT '{}',
    archived_at  timestamptz,
    version      integer     NOT NULL DEFAULT 1,
    created_at   timestamptz NOT NULL DEFAULT now(),
    updated_at   timestamptz NOT NULL DEFAULT now(),
    UNIQUE (user_id, id)
);
CREATE TRIGGER swipe_refs_touch BEFORE UPDATE ON swipe_refs FOR EACH ROW EXECUTE FUNCTION touch_updated_at();

CREATE TABLE subjects (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id     uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    name        text        NOT NULL CHECK (length(trim(name)) > 0),
    area_id     uuid,
    archived_at timestamptz,
    version     integer     NOT NULL DEFAULT 1,
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now(),
    UNIQUE (user_id, id),
    UNIQUE (user_id, name),
    FOREIGN KEY (user_id, area_id) REFERENCES areas (user_id, id) ON DELETE SET NULL (area_id)
);
CREATE TRIGGER subjects_touch BEFORE UPDATE ON subjects FOR EACH ROW EXECUTE FUNCTION touch_updated_at();

CREATE TABLE class_notes (
    id           uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id      uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    subject_id   uuid        NOT NULL,
    on_date      date        NOT NULL,
    topic        text        NOT NULL CHECK (length(trim(topic)) > 0),
    topics_md    text        NOT NULL DEFAULT '',
    questions_md text        NOT NULL DEFAULT '',
    version      integer     NOT NULL DEFAULT 1,
    created_at   timestamptz NOT NULL DEFAULT now(),
    updated_at   timestamptz NOT NULL DEFAULT now(),
    UNIQUE (user_id, id),
    FOREIGN KEY (user_id, subject_id) REFERENCES subjects (user_id, id) ON DELETE CASCADE
);
CREATE TRIGGER class_notes_touch BEFORE UPDATE ON class_notes FOR EACH ROW EXECUTE FUNCTION touch_updated_at();

ALTER TABLE tasks ADD FOREIGN KEY (user_id, class_note_id) REFERENCES class_notes (user_id, id) ON DELETE SET NULL (class_note_id);

CREATE TABLE devlogs (
    id         uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id    uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    on_date    date        NOT NULL,
    summary_md text        NOT NULL DEFAULT '',
    version    integer     NOT NULL DEFAULT 1,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (user_id, id),
    UNIQUE (user_id, on_date)
);
CREATE TRIGGER devlogs_touch BEFORE UPDATE ON devlogs FOR EACH ROW EXECUTE FUNCTION touch_updated_at();

CREATE TABLE devlog_sessions (
    id            uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id       uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    devlog_id     uuid        NOT NULL,
    repo          text        NOT NULL DEFAULT '',
    branch        text        NOT NULL DEFAULT '',
    at_time       time,
    changes_md    text        NOT NULL DEFAULT '',
    bugs_md       text        NOT NULL DEFAULT '',
    decisions_md  text        NOT NULL DEFAULT '',
    next_steps_md text        NOT NULL DEFAULT '',
    raw_text      text,
    position      integer     NOT NULL DEFAULT 0,
    version       integer     NOT NULL DEFAULT 1,
    created_at    timestamptz NOT NULL DEFAULT now(),
    updated_at    timestamptz NOT NULL DEFAULT now(),
    UNIQUE (user_id, id),
    FOREIGN KEY (user_id, devlog_id) REFERENCES devlogs (user_id, id) ON DELETE CASCADE
);
CREATE TRIGGER devlog_sessions_touch BEFORE UPDATE ON devlog_sessions FOR EACH ROW EXECUTE FUNCTION touch_updated_at();

CREATE TABLE devlog_session_projects (
    user_id    uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    session_id uuid NOT NULL,
    project_id uuid NOT NULL,
    PRIMARY KEY (session_id, project_id),
    FOREIGN KEY (user_id, session_id) REFERENCES devlog_sessions (user_id, id) ON DELETE CASCADE,
    FOREIGN KEY (user_id, project_id) REFERENCES projects (user_id, id) ON DELETE CASCADE
);

CREATE TABLE adrs (
    id               uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id          uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    project_id       uuid        NOT NULL,
    number           integer     NOT NULL CHECK (number > 0),
    title            text        NOT NULL CHECK (length(trim(title)) > 0),
    status           text        NOT NULL DEFAULT 'proposed' CHECK (status IN ('proposed', 'accepted', 'superseded')),
    superseded_by_id uuid,
    decided_on       date        NOT NULL,
    context_md       text        NOT NULL DEFAULT '',
    decision_md      text        NOT NULL DEFAULT '',
    alternatives_md  text        NOT NULL DEFAULT '',
    consequences_md  text        NOT NULL DEFAULT '',
    version          integer     NOT NULL DEFAULT 1,
    created_at       timestamptz NOT NULL DEFAULT now(),
    updated_at       timestamptz NOT NULL DEFAULT now(),
    UNIQUE (user_id, id),
    UNIQUE (user_id, project_id, number),
    FOREIGN KEY (user_id, project_id) REFERENCES projects (user_id, id) ON DELETE CASCADE,
    FOREIGN KEY (user_id, superseded_by_id) REFERENCES adrs (user_id, id) ON DELETE SET NULL (superseded_by_id),
    CHECK (status <> 'superseded' OR superseded_by_id IS NOT NULL)
);
CREATE TRIGGER adrs_touch BEFORE UPDATE ON adrs FOR EACH ROW EXECUTE FUNCTION touch_updated_at();

-- Triggers de búsqueda.
CREATE TRIGGER notes_search AFTER INSERT OR UPDATE OR DELETE ON notes FOR EACH ROW EXECUTE FUNCTION search_index_trigger('note', 'title', 'body_md,kind,language', '', 'kind');
CREATE TRIGGER projects_search AFTER INSERT OR UPDATE OR DELETE ON projects FOR EACH ROW EXECUTE FUNCTION search_index_trigger('project', 'name', 'description,notes_md,repo_url', '', 'status');
CREATE TRIGGER areas_search AFTER INSERT OR UPDATE OR DELETE ON areas FOR EACH ROW EXECUTE FUNCTION search_index_trigger('area', 'name', 'description', '', '');
CREATE TRIGGER goals_search AFTER INSERT OR UPDATE OR DELETE ON goals FOR EACH ROW EXECUTE FUNCTION search_index_trigger('goal', 'title', 'motivation,success_criteria,period_key', 'starts_on', 'status');
CREATE TRIGGER habits_search AFTER INSERT OR UPDATE OR DELETE ON habits FOR EACH ROW EXECUTE FUNCTION search_index_trigger('habit', 'name', 'description', '', 'kind');
CREATE TRIGGER people_search AFTER INSERT OR UPDATE OR DELETE ON people FOR EACH ROW EXECUTE FUNCTION search_index_trigger('person', 'name', 'org,role,context_md,relationship', 'last_contact_on', 'relationship');
CREATE TRIGGER tasks_search AFTER INSERT OR UPDATE OR DELETE ON tasks FOR EACH ROW EXECUTE FUNCTION search_index_trigger('task', 'title', 'notes_md', 'planned_on', 'status');
CREATE TRIGGER inbox_search AFTER INSERT OR UPDATE OR DELETE ON inbox_items FOR EACH ROW EXECUTE FUNCTION search_index_trigger('inbox', 'text', '', '', 'status');
CREATE TRIGGER weekly_search AFTER INSERT OR UPDATE OR DELETE ON weekly_reviews FOR EACH ROW EXECUTE FUNCTION search_index_trigger('weekly_review', 'iso_week', 'what_happened,what_didnt,went_well,went_badly,content_md,areas_md,people_md', 'week_start', '');
CREATE TRIGGER devotionals_search AFTER INSERT OR UPDATE OR DELETE ON devotionals FOR EACH ROW EXECUTE FUNCTION search_index_trigger('devotional', 'passage', 'learned,teaching,prayer,song,verse_actions', 'on_date', 'mode');
CREATE TRIGGER readings_search AFTER INSERT OR UPDATE OR DELETE ON readings FOR EACH ROW EXECUTE FUNCTION search_index_trigger('reading', 'title', 'author,ideas_md,quotes_md,application_md', 'finished_on', 'status');
CREATE TRIGGER content_search AFTER INSERT OR UPDATE OR DELETE ON content_pieces FOR EACH ROW EXECUTE FUNCTION search_index_trigger('content', 'title', 'idea,hooks,script_hook,script_body,script_cta,pillar', 'publish_on', 'status');
CREATE TRIGGER swipe_search AFTER INSERT OR UPDATE OR DELETE ON swipe_refs FOR EACH ROW EXECUTE FUNCTION search_index_trigger('swipe', 'title', 'url,creator,why_it_works,hook', '', '');
CREATE TRIGGER subjects_search AFTER INSERT OR UPDATE OR DELETE ON subjects FOR EACH ROW EXECUTE FUNCTION search_index_trigger('subject', 'name', '', '', '');
CREATE TRIGGER class_notes_search AFTER INSERT OR UPDATE OR DELETE ON class_notes FOR EACH ROW EXECUTE FUNCTION search_index_trigger('class_note', 'topic', 'topics_md,questions_md', 'on_date', '');
CREATE TRIGGER devlog_sessions_search AFTER INSERT OR UPDATE OR DELETE ON devlog_sessions FOR EACH ROW EXECUTE FUNCTION search_index_trigger('devlog_session', 'repo', 'branch,changes_md,bugs_md,decisions_md,next_steps_md', '', '');
CREATE TRIGGER adrs_search AFTER INSERT OR UPDATE OR DELETE ON adrs FOR EACH ROW EXECUTE FUNCTION search_index_trigger('adr', 'title', 'context_md,decision_md,alternatives_md,consequences_md', 'decided_on', 'status');
CREATE TRIGGER drawings_search AFTER INSERT OR UPDATE OR DELETE ON drawings FOR EACH ROW EXECUTE FUNCTION search_index_trigger('drawing', 'title', '', '', '');
