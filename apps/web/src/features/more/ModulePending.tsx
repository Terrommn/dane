// Ruta registrada cuyo módulo se integra en la fase 2. No es una pantalla terminada: lo dice explícitamente.
import { Link } from "react-router";
import { Card, PageHeader } from "../../components/ui/Display";

export function ModulePending({ title, includes }: { title: string; includes: string[] }) {
  return (
    <div>
      <PageHeader title={title} />
      <Card>
        <p className="font-semibold">Este módulo se está integrando.</p>
        <p className="mt-1 text-sm text-muted">Aún no está disponible en esta versión. Incluirá:</p>
        <ul className="mt-2 list-disc space-y-1 pl-5 text-sm">
          {includes.map((i) => (
            <li key={i}>{i}</li>
          ))}
        </ul>
        <p className="mt-4 text-sm">
          <Link to="/mas">Ver todos los módulos</Link>
        </p>
      </Card>
    </div>
  );
}
