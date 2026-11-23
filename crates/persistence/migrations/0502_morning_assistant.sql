-- Asistente diario: mensaje de la mañana (check-in guiado) y avisos de eventos de la agenda.
-- Por qué columnas nuevas y no reutilizar daily_time: el recordatorio diario existente es el CIERRE de la noche
-- (se cancela al cerrar el día); la mañana es un segundo momento con su propia hora y su propio interruptor.
ALTER TABLE user_settings
    ADD COLUMN morning_enabled         boolean NOT NULL DEFAULT true,
    ADD COLUMN morning_time            time    NOT NULL DEFAULT '07:00',
    -- Minutos de anticipación para avisar de un evento con hora (0 = sin avisos). Reemplaza la app de Recordatorios.
    ADD COLUMN event_reminder_minutes  smallint NOT NULL DEFAULT 15 CHECK (event_reminder_minutes BETWEEN 0 AND 240);
