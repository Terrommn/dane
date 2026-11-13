-- Datos demo opt-in (D-024): SOLO para usuarios con users.is_demo = true. Usa los ejemplos del vault
-- (70 kg, 4 ejercicios/semana, 5 fallas devocionales, presupuesto de Negocio) como datos ficticios claramente marcados.
CREATE OR REPLACE FUNCTION seed_demo(p_user uuid, p_today date) RETURNS void LANGUAGE plpgsql AS $$
DECLARE
    v_demo boolean;
    h_dev uuid; h_ex uuid; h_read uuid; h_weigh uuid; h_write uuid;
    g_weight uuid; g_books uuid; g_week uuid;
    a_personal uuid; a_trabajo uuid; a_negocio uuid; a_escuela uuid;
    p_negocio uuid;
    acc_debit uuid; acc_card uuid;
    c_comida uuid; c_gas uuid; c_subs uuid; c_negocio uuid; c_deuda uuid; c_sueldo uuid;
    bm uuid; person uuid; book uuid; subj uuid; d date; i int;
    month_start date := date_trunc('month', p_today)::date;
BEGIN
    SELECT is_demo INTO v_demo FROM users WHERE id = p_user;
    IF NOT coalesce(v_demo, false) THEN
        RAISE EXCEPTION 'seed_demo solo se permite en usuarios demo';
    END IF;

    UPDATE user_settings SET weight_goal_kg = 70, weight_direction = 'decrease', max_devotional_failures = 5,
        onboarding_completed_at = now() WHERE user_id = p_user;

    SELECT id INTO a_personal FROM areas WHERE user_id = p_user AND slug = 'personal';
    SELECT id INTO a_trabajo FROM areas WHERE user_id = p_user AND slug = 'trabajo';
    SELECT id INTO a_negocio FROM areas WHERE user_id = p_user AND slug = 'negocio';
    SELECT id INTO a_escuela FROM areas WHERE user_id = p_user AND slug = 'escuela';
    SELECT id INTO p_negocio FROM projects WHERE user_id = p_user AND slug = 'negocio';
    UPDATE projects SET description = '[DEMO] Negocio de ejemplo con presupuesto por rubros' WHERE id = p_negocio;

    -- Hábitos fijos (devocional diario, ejercicio 4/semana, lectura diaria) y de objetivos.
    INSERT INTO habits (user_id, name, kind, fixed_key, created_on, area_id) VALUES (p_user, 'Devocional', 'fixed', 'devotional', p_today - 45, a_personal) RETURNING id INTO h_dev;
    INSERT INTO habits (user_id, name, kind, fixed_key, created_on, area_id) VALUES (p_user, 'Ejercicio', 'fixed', 'exercise', p_today - 45, a_personal) RETURNING id INTO h_ex;
    INSERT INTO habits (user_id, name, kind, fixed_key, created_on, area_id, unit) VALUES (p_user, 'Lectura', 'fixed', 'reading', p_today - 45, a_personal, 'min') RETURNING id INTO h_read;
    INSERT INTO habit_versions (user_id, habit_id, valid_from, frequency) VALUES (p_user, h_dev, p_today - 45, 'daily');
    INSERT INTO habit_versions (user_id, habit_id, valid_from, frequency, times_per_week) VALUES (p_user, h_ex, p_today - 45, 'times_per_week', 4);
    INSERT INTO habit_versions (user_id, habit_id, valid_from, frequency, target_quantity) VALUES (p_user, h_read, p_today - 45, 'daily', 20);

    INSERT INTO goals (user_id, kind, period_key, starts_on, ends_on, title, area_id, motivation, success_criteria,
        progress_type, unit, baseline, target, direction, progress_source)
    VALUES (p_user, 'month', to_char(p_today, 'YYYY-MM'), month_start, (month_start + interval '1 month - 1 day')::date,
        '[DEMO] Bajar a 70 kg', a_personal, 'Tener más energía', 'Pesar 70 kg al cierre del mes', 'current_value', 'kg', 74, 70, 'decrease', 'weight')
    RETURNING id INTO g_weight;
    INSERT INTO goals (user_id, kind, period_key, starts_on, ends_on, title, area_id, motivation, success_criteria,
        progress_type, unit, baseline, target, direction, progress_source)
    VALUES (p_user, 'month', to_char(p_today, 'YYYY-MM'), month_start, (month_start + interval '1 month - 1 day')::date,
        '[DEMO] Leer 4 libros', a_personal, 'Aprender más', '4 libros terminados', 'cumulative', 'libros', 0, 4, 'increase', 'readings')
    RETURNING id INTO g_books;
    INSERT INTO goals (user_id, kind, period_key, starts_on, ends_on, title, area_id, parent_goal_id, motivation, success_criteria)
    VALUES (p_user, 'week', to_char(p_today, 'IYYY-"W"IW'), p_today - (extract(isodow FROM p_today)::int - 1), p_today + (7 - extract(isodow FROM p_today)::int),
        '[DEMO] Terminar el libro actual', a_personal, g_books, 'Avanzar en la meta del mes', 'Libro terminado')
    RETURNING id INTO g_week;

    INSERT INTO habits (user_id, name, kind, goal_id, created_on) VALUES (p_user, 'Pesarme en la mañana', 'goal', g_weight, month_start) RETURNING id INTO h_weigh;
    INSERT INTO habits (user_id, name, kind, goal_id, created_on) VALUES (p_user, 'Escribir 1 cosa en "Lo que hice bien hoy"', 'goal', g_week, p_today - 6) RETURNING id INTO h_write;
    INSERT INTO habit_versions (user_id, habit_id, valid_from, frequency) VALUES (p_user, h_weigh, month_start, 'daily'), (p_user, h_write, p_today - 6, 'daily');
    INSERT INTO habit_links (user_id, trigger, habit_id) VALUES (p_user, 'weight_logged', h_weigh), (p_user, 'positive_reflection', h_write), (p_user, 'devotional_content', h_dev);

    -- 30 días de registros deterministas (algunos días sin registro a propósito).
    FOR i IN 1..30 LOOP
        d := p_today - i;
        IF i % 9 <> 0 THEN
            INSERT INTO daily_checkins (user_id, on_date, weight_kg, energy, evening_done_at)
            VALUES (p_user, d, CASE WHEN i % 3 = 0 THEN NULL ELSE round((72.0 + i * 0.07)::numeric, 1) END, 2 + (i % 4), now());
            INSERT INTO habit_entries (user_id, habit_id, on_date, status, source)
            VALUES (p_user, h_dev, d, CASE WHEN i % 7 = 0 THEN 'not_done' ELSE 'done' END, 'checkin');
            IF extract(isodow FROM d) IN (1, 3, 5, 6) THEN
                INSERT INTO habit_entries (user_id, habit_id, on_date, status, source) VALUES (p_user, h_ex, d, 'done', 'checkin');
            END IF;
            INSERT INTO habit_entries (user_id, habit_id, on_date, status, quantity, source)
            VALUES (p_user, h_read, d, CASE WHEN i % 5 = 0 THEN 'not_done' ELSE 'done' END, CASE WHEN i % 5 = 0 THEN NULL ELSE 20 + (i % 3) * 10 END, 'checkin');
            IF d >= month_start AND i % 3 <> 0 THEN
                INSERT INTO habit_entries (user_id, habit_id, on_date, status, source) VALUES (p_user, h_weigh, d, 'done', 'weight');
            END IF;
        END IF;
    END LOOP;
    INSERT INTO reflection_items (user_id, on_date, polarity, text) VALUES
        (p_user, p_today - 1, 'good', '[DEMO] Hice ejercicio antes del trabajo'),
        (p_user, p_today - 1, 'bad', '[DEMO] Me dormí tarde viendo el celular');
    INSERT INTO daily_log_entries (user_id, on_date, text) VALUES (p_user, p_today - 1, '[DEMO] Reunión de planeación del negocio');
    INSERT INTO devotionals (user_id, on_date, passage, verse_actions, learned, prayer)
    VALUES (p_user, p_today - 1, 'Juan 15:1-8', '[{"verse": "Juan 15:5", "action": "Empezar el día con oración"}]', '[DEMO] Permanecer da fruto.', '[DEMO] Gracias.');

    -- Personas, tareas, lectura, contenido, escuela.
    INSERT INTO people (user_id, name, aliases, area_id, relationship, org, role, context_md, birthday_month, birthday_day, last_contact_on)
    VALUES (p_user, '[DEMO] Ana López', '{Anita}', a_trabajo, 'compañero', 'Empresa Demo', 'Diseño', 'Compañera de proyecto (ficticia).', 3, 14, p_today - 12)
    RETURNING id INTO person;
    INSERT INTO interactions (user_id, person_id, on_date, topic) VALUES (p_user, person, p_today - 12, '[DEMO] Revisión del prototipo');
    INSERT INTO tasks (user_id, title, area_id, planned_on, priority) VALUES
        (p_user, '[DEMO] Llamar al dentista', a_personal, p_today, 3),
        (p_user, '[DEMO] Terminar reporte semanal', a_trabajo, p_today - 1, 2),
        (p_user, '[DEMO] Revisar campaña de Ads', a_negocio, p_today, 2),
        (p_user, '[DEMO] Entregar tarea de cálculo', a_escuela, p_today + 2, 1);
    INSERT INTO task_recurrence_rules (user_id, title, area_id, day_kind, starts_on) VALUES (p_user, '[DEMO] Facturar a cliente', a_negocio, 'penultimate', month_start);
    INSERT INTO readings (user_id, title, author, status, rating, started_on, finished_on)
    VALUES (p_user, '[DEMO] Hábitos atómicos', 'James Clear', 'finished', 5, p_today - 20, p_today - 3) RETURNING id INTO book;
    INSERT INTO entity_links (user_id, src_type, src_id, dst_type, dst_id, relation) VALUES (p_user, 'reading', book, 'goal', g_books, 'contributes_to');
    INSERT INTO goal_progress_events (user_id, goal_id, occurred_on, amount, source_type, source_id) VALUES (p_user, g_books, p_today - 3, 1, 'reading', book);
    INSERT INTO readings (user_id, title, author, status, started_on) VALUES (p_user, '[DEMO] Deep Work', 'Cal Newport', 'reading', p_today - 2);
    INSERT INTO content_pieces (user_id, title, idea, status, hooks, chosen_hook, publish_on, published_on, views, retention_pct, platforms)
    VALUES (p_user, '[DEMO] 3 hábitos para tu mañana', 'Rutina simple', 'published', '{"¿Tu mañana te gana?","3 hábitos en 3 minutos"}', 1, p_today - 5, p_today - 5, 12400, 41.5, '{instagram,tiktok}'),
           (p_user, '[DEMO] Cómo planeo mi semana', 'Planeación', 'script', '{"Planea en 10 minutos"}', 0, p_today + 4, NULL, NULL, NULL, '{instagram}');
    INSERT INTO subjects (user_id, name, area_id) VALUES (p_user, '[DEMO] Cálculo', a_escuela) RETURNING id INTO subj;
    INSERT INTO class_notes (user_id, subject_id, on_date, topic, topics_md) VALUES (p_user, subj, p_today - 2, 'Derivadas', '- Regla de la cadena');
    INSERT INTO weekly_reviews (user_id, iso_week, week_start, peace, what_happened, priorities, completed_at)
    VALUES (p_user, to_char(p_today - 7, 'IYYY-"W"IW'), (p_today - 7) - (extract(isodow FROM p_today - 7)::int - 1), 4,
            '[DEMO] Semana estable', '[{"text": "Terminar libro"}, {"text": "Ejercicio 4 veces"}, {"text": "Campaña de Ads"}]', now());

    -- Finanzas (ejemplos del vault, solo demo).
    SELECT id INTO c_comida FROM fin_categories WHERE user_id = p_user AND kind = 'expense' AND name = 'Comida';
    SELECT id INTO c_gas FROM fin_categories WHERE user_id = p_user AND kind = 'expense' AND name = 'Gasolina';
    SELECT id INTO c_subs FROM fin_categories WHERE user_id = p_user AND kind = 'expense' AND name = 'Suscripciones';
    SELECT id INTO c_negocio FROM fin_categories WHERE user_id = p_user AND kind = 'expense' AND name = 'Negocio';
    SELECT id INTO c_deuda FROM fin_categories WHERE user_id = p_user AND kind = 'expense' AND name = 'Pago de deuda';
    SELECT id INTO c_sueldo FROM fin_categories WHERE user_id = p_user AND kind = 'income' AND name = 'Sueldo';
    INSERT INTO fin_accounts (user_id, name, bank, kind, is_default_debit) VALUES (p_user, '[DEMO] Débito', 'Banco Demo', 'debit', true) RETURNING id INTO acc_debit;
    INSERT INTO fin_accounts (user_id, name, bank, kind, credit_limit_cents, cut_day, pay_day) VALUES (p_user, '[DEMO] Tarjeta A', 'Banco Demo', 'credit_card', 2000000, 5, 25) RETURNING id INTO acc_card;
    INSERT INTO balance_snapshots (user_id, account_id, observed_at, observed_on, balance_cents) VALUES
        (p_user, acc_debit, (month_start::timestamp AT TIME ZONE 'America/Monterrey'), month_start, 1500000),
        (p_user, acc_card, (month_start::timestamp AT TIME ZONE 'America/Monterrey'), month_start, 1500000);
    INSERT INTO transactions (user_id, kind, occurred_on, concept, amount_cents, category_id, account_id) VALUES
        (p_user, 'income', month_start, '[DEMO] Sueldo quincena 1', 1800000, c_sueldo, acc_debit),
        (p_user, 'expense', greatest(month_start, p_today - 6), '[DEMO] Supermercado', 125050, c_comida, acc_card),
        (p_user, 'expense', greatest(month_start, p_today - 4), '[DEMO] Gasolina', 85000, c_gas, acc_debit),
        (p_user, 'expense', greatest(month_start, p_today - 2), '[DEMO] Ads de negocio', 230000, c_negocio, acc_card),
        (p_user, 'expense', greatest(month_start, p_today - 1), '[DEMO] Cena', 45000, c_comida, acc_card);
    INSERT INTO transactions (user_id, kind, occurred_on, concept, amount_cents, category_id, account_id, counter_account_id)
    VALUES (p_user, 'card_payment', greatest(month_start, p_today - 1), '[DEMO] Pago Tarjeta A', 100000, c_deuda, acc_debit, acc_card);
    INSERT INTO fin_recurring_rules (user_id, kind, name, amount_cents, day_kind, day, category_id, account_id, starts_on)
    VALUES (p_user, 'subscription', '[DEMO] Netflix', 29900, 'day', 12, c_subs, acc_card, month_start),
           (p_user, 'fixed_income', '[DEMO] Sueldo', 1800000, 'last', NULL, c_sueldo, acc_debit, month_start);
    INSERT INTO budget_months (user_id, month) VALUES (p_user, to_char(p_today, 'YYYY-MM')) RETURNING id INTO bm;
    INSERT INTO budget_lines (user_id, budget_month_id, category_id, amount_cents) VALUES
        (p_user, bm, c_comida, 600000), (p_user, bm, c_gas, 250000), (p_user, bm, c_subs, 60000), (p_user, bm, c_negocio, 780000);
    INSERT INTO budget_items (user_id, budget_month_id, project_id, category_id, name, amount_cents, keywords, position) VALUES
        (p_user, bm, p_negocio, c_negocio, 'Servidores', 30000, '{Render,servidor,hosting}', 0),
        (p_user, bm, p_negocio, c_negocio, 'WhatsApp', 100000, '{WhatsApp}', 1),
        (p_user, bm, p_negocio, c_negocio, 'Ads', 650000, '{Ads,anuncio,Facebook,Instagram}', 2);
    UPDATE credit_plans SET goal_text = '[DEMO] Subir score sin deuda', rules = '[{"text": "Pagar siempre el total"}, {"text": "Uso < 30%"}]' WHERE user_id = p_user;
    INSERT INTO inbox_items (user_id, text) VALUES (p_user, '[DEMO] Idea: video sobre planeación semanal');
    INSERT INTO notes (user_id, kind, title, body_md, tags) VALUES (p_user, 'til', '[DEMO] SKIP LOCKED', 'Permite colas en PostgreSQL sin bloqueo.', '{lang/sql}');
END $$;
