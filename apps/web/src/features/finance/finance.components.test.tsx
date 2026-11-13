import { cleanup, render, screen, within } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { afterEach, describe, expect, it, vi } from "vitest";
import { AccountCard } from "./AccountsPage";
import { SuggestionResult } from "./CreditPlanPage";
import type { AccountView, CardSuggestion, Transaction } from "./financeApi";
import { TxList } from "./TransactionsPage";

afterEach(cleanup);

describe("SuggestionResult", () => {
  it("explica la tarjeta elegida, marca provisional sin monto y muestra motivos de exclusión", () => {
    const s: CardSuggestion = {
      date: "2026-11-12",
      card_name: "Tarjeta A",
      card_id: "a",
      provisional: true,
      explanation: "Más días hasta su próximo corte (25).",
      max_usage_pct: 30,
      cards: [
        {
          account_id: "a",
          name: "Tarjeta A",
          eligible: true,
          reasons: [],
          days_to_cut: 25,
          next_cut_on: "2026-12-07",
          available_cents: 450000,
        },
        { account_id: "b", name: "Tarjeta B", eligible: false, reasons: ["Hoy es su día de corte"] },
      ],
    };
    render(<SuggestionResult s={s} />);
    expect(screen.getByText("Tarjeta A", { selector: "p" })).toBeTruthy();
    expect(screen.getByText("Provisional")).toBeTruthy();
    expect(screen.getByText("Hoy es su día de corte")).toBeTruthy();
    expect(screen.getByText(/no garantía financiera/)).toBeTruthy();
    expect(screen.getByText("$4,500.00")).toBeTruthy();
  });

  it("sin tarjetas candidatas sugiere débito", () => {
    render(
      <SuggestionResult
        s={{
          date: "2026-11-12",
          debit_account_name: "Débito Banco",
          provisional: false,
          explanation: "Ninguna tarjeta pasa las reglas.",
          max_usage_pct: 30,
          cards: [],
        }}
      />,
    );
    expect(screen.getByText("Débito Banco", { exact: false })).toBeTruthy();
  });
});

describe("AccountCard", () => {
  const base: AccountView = {
    account: {
      id: "c",
      name: "Tarjeta A",
      bank: "Banco",
      kind: "credit_card",
      currency: "MXN",
      credit_limit_cents: 1000000,
      cut_day: 7,
      pay_day: 27,
      status: "active",
      is_default_debit: false,
      notes_md: "",
      version: 1,
      updated_at: "",
    },
    snapshot: {
      id: "s",
      account_id: "c",
      observed_at: "2026-10-01T15:00:00Z",
      observed_on: "2026-10-01",
      balance_cents: 800000,
      semantics: "instant",
      note: "",
      created_at: "",
    },
    estimated_cents: 650000,
    snapshot_age_days: 42,
    used_cents: 350000,
    usage_pct: 35,
    next_cut_on: "2026-12-07",
    next_pay_on: "2026-11-27",
    days_to_pay: 15,
  };

  it("muestra disponible estimado, antigüedad y aviso de conciliación sin afirmar conexión bancaria", () => {
    render(<AccountCard v={base} onEdit={vi.fn()} onSnapshot={vi.fn()} onHistory={vi.fn()} />);
    expect(screen.getByText("$6,500.00")).toBeTruthy();
    expect(screen.getByText(/no es un dato del banco/)).toBeTruthy();
    expect(screen.getByText(/hace 42 días/)).toBeTruthy();
    expect(screen.getByText(/concilia para mejorar el estimado/)).toBeTruthy();
    expect(screen.getByRole("progressbar", { name: "Uso de la línea" }).getAttribute("aria-valuenow")).toBe(
      "35",
    );
  });

  it("sin saldo observado no inventa un estimado", () => {
    render(
      <AccountCard
        v={{
          account: { ...base.account, kind: "debit", credit_limit_cents: null, cut_day: null, pay_day: null },
        }}
        onEdit={vi.fn()}
        onSnapshot={vi.fn()}
        onHistory={vi.fn()}
      />,
    );
    expect(screen.getByText("Sin saldo observado")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Registrar saldo" })).toBeTruthy();
  });
});

describe("TxList", () => {
  const tx = (p: Partial<Transaction>): Transaction => ({
    id: "t",
    kind: "expense",
    status: "confirmed",
    origin: "manual",
    occurred_on: "2026-11-12",
    concept: "Gasolina",
    amount_cents: 50000,
    currency: "MXN",
    notes_md: "",
    version: 1,
    created_at: "",
    updated_at: "",
    ...p,
  });

  it("muestra signo por tipo, transferencias neutrales y pendientes marcadas", () => {
    render(
      <MemoryRouter>
        <TxList
          items={[
            tx({ id: "1" }),
            tx({ id: "2", kind: "income", concept: "Sueldo", amount_cents: 1500000 }),
            tx({ id: "3", kind: "transfer", concept: "Ahorro", account_id: "a", counter_account_id: "b" }),
            tx({ id: "4", status: "pending_reconciliation", concept: "Netflix", origin: "recurring" }),
          ]}
          accName={(id) => (id === "a" ? "Débito" : id === "b" ? "Ahorro" : undefined)}
          catName={() => undefined}
          onOpen={vi.fn()}
        />
      </MemoryRouter>,
    );
    const cards = screen.getByRole("list", { name: "Movimientos" });
    expect(within(cards).getAllByText("−$500.00").length).toBeGreaterThan(0);
    expect(within(cards).getByText("$15,000.00")).toBeTruthy();
    expect(within(cards).getByText("Débito → Ahorro")).toBeTruthy();
    expect(within(cards).getByText("Pendiente de conciliar")).toBeTruthy();
  });
});
