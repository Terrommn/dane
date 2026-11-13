// Datos de finanzas sobre el cliente generado. Todas las mutaciones pasan por la capa de API (Idempotency-Key,
// CSRF/Bearer) e invalidan el espacio `["finance"]` para que dashboard, cuentas y movimientos se recalculen.
import { keepPreviousData, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, newIdempotencyKey, type Schemas } from "../../lib/api";
import { unwrap } from "../../lib/errors";
import type { TxBody } from "./txForm";

export type Category = Schemas["Category"];
export type AccountView = Schemas["AccountView"];
export type Account = Schemas["Account"];
export type Transaction = Schemas["Transaction"];
export type TxPage = Schemas["TxPage"];
export type FinanceDashboard = Schemas["FinanceDashboard"];
export type BudgetView = Schemas["BudgetView"];
export type BudgetItem = Schemas["BudgetItem"];
export type RecurringRule = Schemas["RecurringRule"];
export type OccurrenceView = Schemas["OccurrenceView"];
export type DebtView = Schemas["DebtView"];
export type CreditPlan = Schemas["CreditPlan"];
export type CardSuggestion = Schemas["CardSuggestionView"];
export type ProjectReport = Schemas["ProjectReport"];
export type BalanceSnapshot = Schemas["BalanceSnapshot"];

/**
 * Cuerpo real de `POST/PUT /finance/recurring`. El OpenAPI actual publica `RuleBody` con la forma de las reglas de
 * tareas por una colisión de nombres en el servidor; esta es la forma que acepta el handler de finanzas
 * (`apps/server/src/api/finance/recurring.rs`).
 */
export interface FinanceRuleBody {
  kind: "subscription" | "fixed_income";
  name: string;
  amount_cents: number;
  currency?: string | null;
  day_kind: "day" | "last" | "penultimate";
  day?: number | null;
  category_id?: string | null;
  account_id?: string | null;
  area_id?: string | null;
  project_id?: string | null;
  status: "active" | "cancelled" | "ended";
  starts_on?: string | null;
  ends_on?: string | null;
  notes_md?: string;
  version?: number | null;
}

export interface TxFilters {
  from?: string;
  to?: string;
  kind?: string;
  status?: string;
  category_id?: string;
  account_id?: string;
  project_id?: string;
  person_id?: string;
  debt_id?: string;
  q?: string;
  sort?: "date_desc" | "date_asc" | "amount_desc" | "amount_asc";
  limit?: number;
  offset?: number;
}

const clean = <T extends object>(o: T): T =>
  Object.fromEntries(Object.entries(o).filter(([, v]) => v !== "" && v !== undefined && v !== null)) as T;

export function useCategories(includeArchived = false) {
  return useQuery({
    queryKey: ["finance", "categories", includeArchived],
    queryFn: async () =>
      unwrap(await api.GET("/finance/categories", { params: { query: { archived: includeArchived } } })),
  });
}

export function useAccounts(includeArchived = false) {
  return useQuery({
    queryKey: ["finance", "accounts", includeArchived],
    queryFn: async () =>
      unwrap(await api.GET("/finance/accounts", { params: { query: { archived: includeArchived } } })),
  });
}

export function useSnapshots(accountId: string | null) {
  return useQuery({
    enabled: !!accountId,
    queryKey: ["finance", "snapshots", accountId],
    queryFn: async () =>
      unwrap(
        await api.GET("/finance/accounts/{id}/snapshots", { params: { path: { id: accountId ?? "" } } }),
      ),
  });
}

export function useTransactions(filters: TxFilters) {
  return useQuery({
    queryKey: ["finance", "transactions", filters],
    placeholderData: keepPreviousData,
    queryFn: async () =>
      unwrap(await api.GET("/finance/transactions", { params: { query: clean(filters) } })),
  });
}

export function useTransaction(id: string | null) {
  return useQuery({
    enabled: !!id,
    queryKey: ["finance", "transaction", id],
    queryFn: async () =>
      unwrap(await api.GET("/finance/transactions/{id}", { params: { path: { id: id ?? "" } } })),
  });
}

export function useDashboard(month?: string) {
  return useQuery({
    queryKey: ["finance", "dashboard", month ?? "current"],
    queryFn: async () =>
      unwrap(await api.GET("/finance/dashboard", { params: { query: month ? { month } : {} } })),
  });
}

export function useBudget(month: string) {
  return useQuery({
    queryKey: ["finance", "budget", month],
    queryFn: async () => unwrap(await api.GET("/finance/budgets/{month}", { params: { path: { month } } })),
  });
}

export function useRecurring() {
  return useQuery({
    queryKey: ["finance", "recurring"],
    queryFn: async () => unwrap(await api.GET("/finance/recurring")),
  });
}

export function useOccurrences(month?: string) {
  return useQuery({
    queryKey: ["finance", "occurrences", month ?? "current"],
    queryFn: async () =>
      unwrap(await api.GET("/finance/recurring/occurrences", { params: { query: month ? { month } : {} } })),
  });
}

export function useDebts() {
  return useQuery({
    queryKey: ["finance", "debts"],
    queryFn: async () => unwrap(await api.GET("/finance/debts")),
  });
}

export function useCreditPlan() {
  return useQuery({
    queryKey: ["finance", "credit-plan"],
    queryFn: async () => unwrap(await api.GET("/finance/credit-plan")),
  });
}

export function useCardSuggestion(date: string | undefined, amountCents: number | null) {
  return useQuery({
    queryKey: ["finance", "card-suggestion", date ?? "today", amountCents],
    queryFn: async () =>
      unwrap(
        await api.GET("/finance/card-suggestion", {
          params: { query: clean({ date, amount_cents: amountCents ?? undefined }) },
        }),
      ),
  });
}

export function useProjectReport(projectId: string, month?: string) {
  return useQuery({
    queryKey: ["finance", "project-report", projectId, month ?? "current"],
    queryFn: async () =>
      unwrap(
        await api.GET("/finance/projects/{id}/report", {
          params: { path: { id: projectId }, query: month ? { month } : {} },
        }),
      ),
  });
}

/** Personas y proyectos para selects (módulos ajenos; solo lectura). */
export function usePeopleOptions() {
  return useQuery({
    queryKey: ["people", "options"],
    queryFn: async () => unwrap(await api.GET("/people")),
    staleTime: 60_000,
  });
}

export function useProjectOptions() {
  return useQuery({
    queryKey: ["projects", { includeArchived: false }],
    queryFn: async () => unwrap(await api.GET("/projects", { params: { query: { archived: false } } })),
    staleTime: 60_000,
  });
}

export function useAreaOptions() {
  return useQuery({
    queryKey: ["areas", { includeArchived: false }],
    queryFn: async () => unwrap(await api.GET("/areas", { params: { query: { archived: false } } })),
    staleTime: 60_000,
  });
}

/** Búsqueda de objetivos por texto (para vincular una deuda a un objetivo). */
export function useGoalSearch(q: string) {
  return useQuery({
    enabled: q.trim().length >= 2,
    queryKey: ["search", "goal", q],
    queryFn: async () =>
      unwrap(await api.GET("/search", { params: { query: { q, types: "goal", limit: 10 } } })),
  });
}

function useInvalidateFinance() {
  const qc = useQueryClient();
  return () => qc.invalidateQueries({ queryKey: ["finance"] });
}

export function useTxMutations() {
  const invalidate = useInvalidateFinance();
  const create = useMutation({
    mutationFn: async (body: TxBody) => unwrap(await api.POST("/finance/transactions", { body })),
    onSuccess: invalidate,
  });
  const batch = useMutation({
    mutationFn: async (items: TxBody[]) =>
      unwrap(await api.POST("/finance/transactions/batch", { body: { items } })),
    onSuccess: invalidate,
  });
  const update = useMutation({
    mutationFn: async ({ id, body }: { id: string; body: TxBody }) =>
      unwrap(await api.PUT("/finance/transactions/{id}", { params: { path: { id } }, body })),
    onSuccess: invalidate,
  });
  const voidTx = useMutation({
    mutationFn: async ({ id, version }: { id: string; version: number }) =>
      unwrap(
        await api.POST("/finance/transactions/{id}/void", { params: { path: { id } }, body: { version } }),
      ),
    onSuccess: invalidate,
  });
  const confirm = useMutation({
    mutationFn: async ({ id, body }: { id: string; body: Schemas["ConfirmBody"] }) =>
      unwrap(await api.POST("/finance/transactions/{id}/confirm", { params: { path: { id } }, body })),
    onSuccess: invalidate,
  });
  return { create, batch, update, voidTx, confirm };
}

export function useAccountMutations() {
  const invalidate = useInvalidateFinance();
  const create = useMutation({
    mutationFn: async (body: Schemas["AccountBody"]) => unwrap(await api.POST("/finance/accounts", { body })),
    onSuccess: invalidate,
  });
  const update = useMutation({
    mutationFn: async ({ id, body }: { id: string; body: Schemas["AccountBody"] }) =>
      unwrap(await api.PUT("/finance/accounts/{id}", { params: { path: { id } }, body })),
    onSuccess: invalidate,
  });
  const snapshot = useMutation({
    mutationFn: async ({ id, body }: { id: string; body: Schemas["SnapshotBody"] }) =>
      unwrap(await api.POST("/finance/accounts/{id}/snapshots", { params: { path: { id } }, body })),
    onSuccess: invalidate,
  });
  return { create, update, snapshot };
}

export function accountBody(a: Account, patch: Partial<Schemas["AccountBody"]> = {}): Schemas["AccountBody"] {
  return {
    name: a.name,
    bank: a.bank,
    kind: a.kind,
    currency: a.currency,
    credit_limit_cents: a.credit_limit_cents ?? null,
    cut_day: a.cut_day ?? null,
    pay_day: a.pay_day ?? null,
    status: a.status,
    is_default_debit: a.is_default_debit,
    notes_md: a.notes_md,
    archived: a.archived_at != null,
    ...patch,
    version: a.version,
  };
}

export function useCategoryMutations() {
  const invalidate = useInvalidateFinance();
  const create = useMutation({
    mutationFn: async (body: Schemas["CategoryBody"]) =>
      unwrap(await api.POST("/finance/categories", { body })),
    onSuccess: invalidate,
  });
  const update = useMutation({
    mutationFn: async ({ id, body }: { id: string; body: Schemas["CategoryBody"] }) =>
      unwrap(await api.PUT("/finance/categories/{id}", { params: { path: { id } }, body })),
    onSuccess: invalidate,
  });
  return { create, update };
}

export function useBudgetMutation(month: string) {
  const invalidate = useInvalidateFinance();
  return useMutation({
    mutationFn: async (body: Schemas["BudgetBody"]) =>
      unwrap(await api.PUT("/finance/budgets/{month}", { params: { path: { month } }, body })),
    onSuccess: invalidate,
  });
}

export function useRecurringMutations() {
  const invalidate = useInvalidateFinance();
  const create = useMutation({
    mutationFn: async (body: FinanceRuleBody) => unwrap(await api.POST("/finance/recurring", { body })),
    onSuccess: invalidate,
  });
  const update = useMutation({
    mutationFn: async ({ id, body }: { id: string; body: FinanceRuleBody }) =>
      unwrap(
        await api.PUT("/finance/recurring/{id}", {
          params: { path: { id } },
          body,
        }),
      ),
    onSuccess: invalidate,
  });
  const materialize = useMutation({
    mutationFn: async () => unwrap(await api.POST("/finance/recurring/materialize")),
    onSuccess: invalidate,
  });
  return { create, update, materialize };
}

export function ruleBody(r: RecurringRule, patch: Partial<FinanceRuleBody> = {}): FinanceRuleBody {
  return {
    kind: r.kind as FinanceRuleBody["kind"],
    name: r.name,
    amount_cents: r.amount_cents,
    currency: r.currency,
    day_kind: r.day_kind as FinanceRuleBody["day_kind"],
    day: r.day ?? null,
    category_id: r.category_id ?? null,
    account_id: r.account_id ?? null,
    area_id: r.area_id ?? null,
    project_id: r.project_id ?? null,
    status: r.status as FinanceRuleBody["status"],
    starts_on: r.starts_on,
    ends_on: r.ends_on ?? null,
    notes_md: r.notes_md,
    ...patch,
    version: r.version,
  };
}

export function useDebtMutations() {
  const invalidate = useInvalidateFinance();
  const create = useMutation({
    mutationFn: async (body: Schemas["DebtBody"]) => unwrap(await api.POST("/finance/debts", { body })),
    onSuccess: invalidate,
  });
  const update = useMutation({
    mutationFn: async ({ id, body }: { id: string; body: Schemas["DebtBody"] }) =>
      unwrap(await api.PUT("/finance/debts/{id}", { params: { path: { id } }, body })),
    onSuccess: invalidate,
  });
  return { create, update };
}

export function usePlanMutation() {
  const invalidate = useInvalidateFinance();
  return useMutation({
    mutationFn: async (body: Schemas["PlanBody"]) => unwrap(await api.PUT("/finance/credit-plan", { body })),
    onSuccess: invalidate,
  });
}

/** Clave estable por elemento de captura (reintentos sin duplicar; dos compras iguales con claves distintas). */
export const newClientKey = newIdempotencyKey;
