import { zodResolver } from "@hookform/resolvers/zod";
import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { useForm } from "react-hook-form";
import { Navigate, useSearchParams } from "react-router";
import { z } from "zod";
import { Button, buttonClass } from "../../components/ui/Button";
import { Card } from "../../components/ui/Display";
import { TextField } from "../../components/ui/Field";
import { ErrorState, LoadingState } from "../../components/ui/States";
import { api } from "../../lib/api";
import { errorMessage, unwrap } from "../../lib/errors";
import { DemoEntry } from "../demo/Demo";
import { useAuth } from "./AuthProvider";

const ERRORS: Record<string, string> = {
  cancelled: "Cancelaste el inicio de sesión en Telegram.",
  invalid_state: "La solicitud de inicio de sesión venció o ya se usó. Intenta de nuevo.",
  invalid_token: "Telegram no pudo verificar tu identidad. Intenta de nuevo.",
  forbidden: "Esta cuenta de Telegram no tiene acceso a esta app.",
  not_configured: "Telegram Login no está configurado en el servidor.",
  rate_limited: "Demasiados intentos. Espera un minuto.",
  invalid_request: "Solicitud inválida. Intenta de nuevo.",
  internal: "Error del servidor. Intenta más tarde.",
};

/** Ruta interna segura para volver tras iniciar sesión. */
export function safeRedirect(p: string | null): string {
  return p?.startsWith("/") && !p.startsWith("//") && !p.includes("\\") ? p : "/";
}

const devSchema = z.object({
  telegram_id: z.coerce.number().int().positive("Escribe un ID numérico de Telegram"),
});

function DevLogin({ redirectTo }: { redirectTo: string }) {
  const { completeDevLogin } = useAuth();
  const [err, setErr] = useState<string | null>(null);
  const form = useForm<z.input<typeof devSchema>, unknown, z.output<typeof devSchema>>({
    resolver: zodResolver(devSchema),
    defaultValues: { telegram_id: 111 },
  });
  return (
    <form
      className="space-y-3"
      onSubmit={form.handleSubmit(async (v) => {
        setErr(null);
        try {
          const r = unwrap(await api.POST("/auth/dev-login", { body: { telegram_id: v.telegram_id } }));
          await completeDevLogin(r.user_id);
          window.history.replaceState(null, "", redirectTo);
        } catch (e) {
          setErr(errorMessage(e));
        }
      })}
    >
      <TextField
        label="ID de Telegram (solo desarrollo)"
        inputMode="numeric"
        hint="Debe estar en ALLOWED_TELEGRAM_IDS. Este acceso está bloqueado en producción."
        error={form.formState.errors.telegram_id?.message}
        {...form.register("telegram_id")}
      />
      {err && (
        <p role="alert" className="text-sm text-danger">
          {err}
        </p>
      )}
      <Button type="submit" variant="secondary" loading={form.formState.isSubmitting}>
        Entrar en modo desarrollo
      </Button>
    </form>
  );
}

export function LoginPage() {
  const { status, error } = useAuth();
  const [params] = useSearchParams();
  const redirectTo = safeRedirect(params.get("redirect_to"));
  const cfg = useQuery({
    queryKey: ["auth-config"],
    queryFn: async () => unwrap(await api.GET("/auth/config")),
  });

  if (status === "authenticated") return <Navigate to={redirectTo} replace />;
  const errCode = params.get("error");

  return (
    <main
      className="mx-auto flex min-h-screen max-w-md flex-col justify-center gap-4 p-4"
      style={{ paddingTop: "calc(1rem + var(--safe-top))" }}
    >
      <div className="text-center">
        <h1 className="text-3xl font-bold">Dane</h1>
        <p className="mt-1 text-muted">Hábitos, objetivos y organización de tu vida.</p>
      </div>
      {(errCode || error) && (
        <p role="alert" className="rounded-xl border border-danger bg-surface p-3 text-sm text-danger">
          {errCode ? (ERRORS[errCode] ?? "No se pudo iniciar sesión.") : error}
        </p>
      )}
      <Card>
        {cfg.isPending ? (
          <LoadingState rows={1} />
        ) : cfg.isError ? (
          <ErrorState
            error={cfg.error}
            onRetry={() => cfg.refetch()}
            title="No se pudo contactar al servidor"
          />
        ) : (
          <div className="space-y-4">
            {cfg.data.oidc_enabled ? (
              <a
                className={buttonClass("primary", "md", "w-full no-underline")}
                href={`/api/v1/auth/oidc/start?redirect_to=${encodeURIComponent(redirectTo)}`}
              >
                Iniciar sesión con Telegram
              </a>
            ) : (
              <p className="text-sm text-muted">
                El inicio de sesión web con Telegram no está configurado todavía.
              </p>
            )}
            {cfg.data.bot_username && (
              <p className="text-sm text-muted">
                También puedes abrir la app dentro de Telegram desde{" "}
                <a href={`https://t.me/${cfg.data.bot_username}`} target="_blank" rel="noreferrer">
                  @{cfg.data.bot_username}
                </a>
                .
              </p>
            )}
            {cfg.data.dev_login_enabled && (
              <div className="border-t border-line pt-4">
                <DevLogin redirectTo={redirectTo} />
              </div>
            )}
            {cfg.data.dev_login_enabled && (
              <div className="border-t border-line pt-4">
                <DemoEntry redirectTo={redirectTo} />
              </div>
            )}
          </div>
        )}
      </Card>
    </main>
  );
}
