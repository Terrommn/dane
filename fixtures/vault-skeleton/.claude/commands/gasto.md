---
description: Registra uno o varios gastos (texto libre) en la tabla de gastos del mes
argument-hint: <ej. "ayer 250 gasolina con tarjeta, hoy 180 cena con mi pareja">
---
Gastos: $ARGUMENTS

Fecha de hoy en America/Monterrey (`TZ=America/Monterrey date +%F`). Nota del mes: `Proyectos/personal/Finanzas/Gastos/Gastos <YYYY-MM>.md` (el mes de la **fecha del gasto**, no necesariamente el de hoy).

1. Si no hay texto arriba, pídeme los gastos en una línea y espera.
2. Separa el texto en gastos individuales. Para cada uno saca:
   - **Fecha** `YYYY-MM-DD`: resuelve "hoy", "ayer", "el sábado", etc. contra la fecha de hoy. Si no digo, es hoy.
   - **Concepto**: corto y claro ("Gasolina", "Cena con [[Ana López|Ana]]"). Enlaza personas de `Personas/` igual que en `dump`.
   - **Monto**: número en pesos, formato `$1,250` (sin decimales salvo que los diga).
   - **Categoría**, exactamente una de: `Comida`, `Gasolina`, `Casa`, `Salud/gym`, `Deporte`, `Suscripciones`, `Personal/ocio`, `Salidas con amigos`, `Pareja`, `Trabajo`, `Inversión`, `Negocio`, `Nómina`, `Pago de deuda`, `Otro`. Sueldos o pagos a mis empleados → `Nómina`. Compra de cripto, acciones o fondos → `Inversión`. Todo gasto de Negocio (anuncios de Meta/Facebook/Instagram, WhatsApp, servidores como Render) → `Negocio`, y enlaza `[[negocio]]` en el concepto (ej. `Ads de [[negocio]]`). Usa en el concepto la palabra del rubro (`Ads`, `WhatsApp`, `Render`/servidor) para que cuente en el desglose de `## Presupuesto` de `Proyectos/negocio/negocio.md`. Salidas o cosas con mi pareja → `Pareja`; salir con amigos → `Salidas con amigos`. Todo lo de deporte (cuotas, clases, equipo) → `Deporte`.
   - **Pago**: si fue con tarjeta de crédito, **enlaza su nota** de `Proyectos/personal/Finanzas/Tarjetas/`: `[[Tarjeta A\|A]]` (con `\|` porque va dentro de una tabla). Si la tarjeta no tiene nota, pregúntame su límite, disponible, día de corte y día de pago y créala desde `Templates/Tarjeta.md`. Si no, `Efectivo`, `Transferencia`, `Débito <banco>` o `Prestado ([[Persona\|nombre]])`. Si no digo, déjalo vacío.
   Si algo es ambiguo (monto o categoría), pregunta antes de escribir.
3. Si la nota del mes no existe, créala desde `Templates/Gastos.md`: sustituye la sintaxis de Templater (el mes va en `mes:` y en el título) y en `## Presupuesto` copia tal cual la tabla `## Presupuesto` de la nota del mes anterior (`Gastos/Gastos <YYYY-MM anterior>.md`); si no existe, deja la tabla del template con las categorías vacías. Cada mes guarda su propio presupuesto: nunca edites el de un mes pasado salvo que te lo pida.
   Si digo un presupuesto ("presupuesto de comida 3000"), cámbialo solo en la tabla `## Presupuesto` del mes actual.
   Presupuesto por área: si un área tiene su desglose (`## Presupuesto` en `Proyectos/<p>/<p>.md`, ej. Negocio), el presupuesto de esa categoría en el mes es **la suma del desglose**. Si cambio un rubro ("whatsapp de negocio 1500"), cambia el rubro en la nota del proyecto y actualiza el total de la categoría en el mes actual.
4. Agrega cada gasto como fila de la tabla `## Registro` (`| Fecha | Concepto | Monto | Categoría | Pago |`), ordenada por fecha (las del mismo día, en el orden en que las dije). No borres ni cambies filas existentes; si una fila idéntica (misma fecha, concepto y monto) ya existe, no la dupliques y avísame.
5. **Préstamos de familia** (ej. "me prestó mi hermana"): cada gasto se registra normal con `Pago` = `Prestado ([[Persona]])`. **No** es deuda ni va al objetivo de deudas. Cuando le pague a esa persona, **no** lo registres como gasto (ya se contó); si acaso, menciónalo en el Log de la daily.
6. **Pago a una tarjeta**: si le pago a una tarjeta de crédito, el concepto enlaza la tarjeta (`Pago [[Tarjeta A\|A]]`), categoría `Pago de deuda`; así el panel le devuelve ese crédito.
   **Disponible**: si digo cuánto disponible tiene una tarjeta, actualiza `disponible` y `actualizado: <hoy>` en su nota (no toques lo demás).
7. **Pago de deuda**: si es un pago a una tarjeta o préstamo (u otra deuda listada en el objetivo del mes), usa categoría `Pago de deuda` y concepto `Pago <deuda>`. Si con ese pago la deuda queda en $0, marca su checkbox en el objetivo del mes (`Proyectos/personal/Objetivos/Mes <YYYY-MM>.md`) y en el objetivo de la semana si aparece ahí. Si es un abono parcial, no marques nada y dime cuánto falta.
8. **Suscripciones**: si digo que algo es suscripción (o que la cambié/cancelé), agrégala o actualízala en `## Lista` de `Proyectos/personal/Finanzas/Suscripciones.md` (`| Servicio | Monto | Día de cobro | Categoría | Pago | Área | Estado |`, estado `activa` o `cancelada`; nunca borres filas). Si no digo el día de cobro, pregúntalo. Si el cobro de este mes ya pasó, regístralo también en la tabla del mes como `Suscripción <Servicio>` (sin duplicar).
9. Respóndeme en 2 líneas: qué registré (monto y categoría) y el total del mes sin pagos de deuda.
