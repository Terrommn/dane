// Arranque de sesión para las dos superficies con un solo build:
// - Mini App: solo si Telegram aportó initData firmado → POST /auth/telegram/miniapp → Bearer en memoria.
// - Web/PWA: cookie HttpOnly existente → GET /me; si no hay sesión → /login (Telegram Login OIDC).
import {
  createContext,
  type ReactNode,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { queryClient } from "../../app/queryClient";
import { api, type Schemas } from "../../lib/api";
import { ApiError, isNetworkError, toApiError, unwrap } from "../../lib/errors";
import { deleteUserDb } from "../../lib/offline/db";
import { pendingCount } from "../../lib/offline/store";
import { clearSession, getSession, setSession, setUnauthorizedHandler } from "../../lib/session";
import {
  initTelegramUi,
  isTelegramLaunch,
  loadTelegramScript,
  parseStartParam,
  type StartTarget,
  setTelegram,
  type TgWebApp,
} from "../../lib/telegram";

export type AuthStatus = "loading" | "authenticated" | "anonymous" | "error";
export type Me = Schemas["Me"];

interface AuthContextValue {
  status: AuthStatus;
  me: Me | null;
  userId: string | null;
  isTelegram: boolean;
  telegram: TgWebApp | null;
  startTarget: StartTarget | null;
  error: string | null;
  /** Abierta sin conexión con el último usuario del dispositivo (solo datos locales). */
  offline: boolean;
  /** Aviso cuando al cambiar de cuenta se descartaron cambios locales de otra cuenta. */
  notice: string | null;
  refreshMe: () => Promise<void>;
  /** Cierra sesión. Antes de llamar, la UI debe confirmar si hay pendientes (`countPending`). */
  logout: (everywhere?: boolean) => Promise<void>;
  countPending: () => Promise<number>;
  completeDevLogin: (userId: string) => Promise<void>;
}

const AuthContext = createContext<AuthContextValue | null>(null);
const LAST_USER_KEY = "dane:last-user"; // solo el ID (no es secreto); nunca tokens.

function lastUserId(): string | null {
  try {
    return localStorage.getItem(LAST_USER_KEY);
  } catch {
    return null;
  }
}

export function useAuth(): AuthContextValue {
  const ctx = useContext(AuthContext);
  if (!ctx) throw new Error("useAuth fuera de AuthProvider");
  return ctx;
}

/** Limpia todos los datos privados locales de un usuario: IndexedDB y caché de consultas. */
export async function clearLocalUserData(userId: string | null) {
  queryClient.clear();
  if (userId) await deleteUserDb(userId);
  if ("caches" in window) {
    // Por si algún runtime cache guardó algo de la API (no debería: el SW nunca cachea /api).
    for (const k of await caches.keys()) if (/api|private/i.test(k)) await caches.delete(k);
  }
}

async function handleAccountSwitch(userId: string): Promise<string | null> {
  let notice: string | null = null;
  let last: string | null = null;
  try {
    last = localStorage.getItem(LAST_USER_KEY);
  } catch {
    /* almacenamiento bloqueado */
  }
  if (last && last !== userId) {
    const n = await pendingCount(last).catch(() => 0);
    await clearLocalUserData(last);
    if (n > 0) notice = `Se descartaron ${n} cambios sin sincronizar de otra cuenta en este dispositivo.`;
  }
  try {
    localStorage.setItem(LAST_USER_KEY, userId);
  } catch {
    /* almacenamiento bloqueado */
  }
  return notice;
}

export function AuthProvider({ children }: { children: ReactNode }) {
  const [status, setStatus] = useState<AuthStatus>("loading");
  const [me, setMe] = useState<Me | null>(null);
  const [telegram, setTg] = useState<TgWebApp | null>(null);
  const [startTarget, setStartTarget] = useState<StartTarget | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [offline, setOffline] = useState(false);
  const [offlineUserId, setOfflineUserId] = useState<string | null>(null);

  // Al recuperar la conexión tras abrir sin red: revalidar la sesión real (401 → iniciar sesión).
  useEffect(() => {
    if (!offline) return;
    const retry = () => {
      loadMeRef
        .current?.()
        .then(() => setOffline(false))
        .catch((e) => {
          if (e instanceof ApiError && (e.status === 401 || e.status === 403)) setStatus("anonymous");
        });
    };
    window.addEventListener("online", retry);
    return () => window.removeEventListener("online", retry);
  }, [offline]);

  const loadMeRef = useRef<(() => Promise<void>) | null>(null);
  const loadMe = useCallback(async () => {
    const r = await api.GET("/me");
    const data = unwrap(r);
    setMe(data);
    const n = await handleAccountSwitch(data.user.id);
    if (n) setNotice(n);
    setSession({ ...getSession(), userId: data.user.id });
    setStatus("authenticated");
  }, []);

  loadMeRef.current = loadMe;

  const telegramLogin = useCallback(async (wa: TgWebApp) => {
    const r = await api.POST("/auth/telegram/miniapp", { body: { init_data: wa.initData } });
    const s = unwrap(r);
    setSession({ mode: "bearer", token: s.token, userId: s.user_id });
    setStartTarget(parseStartParam(s.start_param));
  }, []);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        if (isTelegramLaunch()) {
          const wa = await loadTelegramScript();
          if (wa) {
            initTelegramUi(wa);
            setTelegram(wa);
            if (!cancelled) setTg(wa);
            await telegramLogin(wa);
            await loadMe();
            return;
          }
        }
        setSession({ mode: "cookie", token: null, userId: null });
        await loadMe();
      } catch (e) {
        if (cancelled) return;
        if (e instanceof ApiError && (e.status === 401 || e.status === 403)) {
          setStatus("anonymous");
          setError(e.status === 403 || e.code !== "unauthorized" ? e.message : null);
        } else if (isNetworkError(e) && lastUserId()) {
          // Sin red (o server inalcanzable): abrir con el último usuario de este dispositivo para ver sus borradores
          // locales. Nada se presenta como guardado en el servidor; al reconectar se revalida la sesión.
          setSession({ ...getSession(), userId: lastUserId() });
          setOfflineUserId(lastUserId());
          setOffline(true);
          setStatus("authenticated");
        } else {
          setStatus("error");
          setError(e instanceof Error ? e.message : "No se pudo iniciar");
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [loadMe, telegramLogin]);

  // Sesión vencida durante el uso: en Mini App se reintenta con initData; en web se pide iniciar sesión.
  useEffect(() => {
    setUnauthorizedHandler(() => {
      const wa = telegram;
      if (wa) {
        telegramLogin(wa).catch(() => {
          setStatus("anonymous");
          setError("La sesión de Telegram venció; vuelve a abrir la app desde el bot.");
        });
      } else {
        setStatus("anonymous");
      }
    });
    return () => setUnauthorizedHandler(null);
  }, [telegram, telegramLogin]);

  const logout = useCallback(async (everywhere = false) => {
    const userId = getSession().userId;
    try {
      const path = everywhere ? "/auth/logout-all" : "/auth/logout";
      const r = await api.POST(path);
      if (!r.response.ok && r.response.status !== 401) throw toApiError(r.response.status, r.error);
    } finally {
      await clearLocalUserData(userId);
      try {
        localStorage.removeItem(LAST_USER_KEY);
      } catch {
        /* ignorar */
      }
      clearSession();
      setMe(null);
      setStatus("anonymous");
    }
  }, []);

  const countPending = useCallback(async () => {
    const id = getSession().userId;
    return id ? pendingCount(id) : 0;
  }, []);

  const completeDevLogin = useCallback(async () => {
    setSession({ mode: "cookie", token: null, userId: null });
    setError(null);
    await loadMe();
  }, [loadMe]);

  const value = useMemo<AuthContextValue>(
    () => ({
      status,
      me,
      userId: me?.user.id ?? offlineUserId,
      isTelegram: telegram !== null,
      telegram,
      startTarget,
      error,
      offline,
      notice,
      refreshMe: loadMe,
      logout,
      countPending,
      completeDevLogin,
    }),
    [
      status,
      me,
      offlineUserId,
      offline,
      telegram,
      startTarget,
      error,
      notice,
      loadMe,
      logout,
      countPending,
      completeDevLogin,
    ],
  );
  return <AuthContext.Provider value={value}>{children}</AuthContext.Provider>;
}
