// Rutas del módulo de finanzas (montadas bajo /finanzas/*).
import { Route, Routes } from "react-router";
import { AccountsPage } from "./AccountsPage";
import { BudgetPage } from "./BudgetPage";
import { CategoriesPage } from "./CategoriesPage";
import { CreditPlanPage } from "./CreditPlanPage";
import { DebtsPage } from "./DebtsPage";
import { FinanceDashboardPage } from "./FinanceDashboardPage";
import { ProjectReportPage } from "./ProjectReportPage";
import { RecurringPage } from "./RecurringPage";
import { TransactionsPage } from "./TransactionsPage";

export function FinanceRoutes() {
  return (
    <Routes>
      <Route index element={<FinanceDashboardPage />} />
      <Route path="movimientos" element={<TransactionsPage />} />
      <Route path="cuentas" element={<AccountsPage />} />
      <Route path="presupuesto" element={<BudgetPage />} />
      <Route path="recurrentes" element={<RecurringPage />} />
      <Route path="deudas" element={<DebtsPage />} />
      <Route path="plan" element={<CreditPlanPage />} />
      <Route path="categorias" element={<CategoriesPage />} />
      <Route path="proyectos/:id" element={<ProjectReportPage />} />
      <Route path="*" element={<FinanceDashboardPage />} />
    </Routes>
  );
}
