---
categories:
  - "[[Cuentas]]"
banco:
saldo:
actualizado: <% tp.date.now("YYYY-MM-DD") %>
estado: activa
---
# <% tp.file.title %>

`saldo` es lo que había en la cuenta al **empezar** el día `actualizado`. El panel le suma los ingresos que entraron a esta cuenta desde ese día y le resta los gastos pagados con ella (`Débito <banco>`, `Transferencia <banco>`), así que todo lo que se registre se refleja solo. Cuando se ajuste al saldo real de la app: `saldo` = saldo real + lo que salió hoy − lo que entró hoy.

## Notas
- 
