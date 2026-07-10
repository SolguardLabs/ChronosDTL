# Seguridad

## Modelo

ChronosDTL asume cuentas registradas, activos configurados por catalogo, pools
con limites de utilizacion y curvas de acumulacion por epoch. Las operaciones
economicas pasan por el ledger principal para mantener eventos, snapshots y
checks de riesgo coherentes.

## Invariantes esperadas

- Una posicion abierta mantiene principal, colateral y pool asociados.
- El pool no presta por encima de la liquidez disponible.
- El colateral queda retenido hasta cierre o expiracion.
- Las cotizaciones de cierre incluyen principal, intereses, penalizaciones y
  cargos visibles.
- Los locks temporales tienen propietario, modo, epoch de liberacion y snapshot.
- Las expiraciones solo se barren despues de la ventana de gracia.

## Validacion

La suite automatizada ejecuta:

```bash
cargo fmt --all -- --check
cargo build --all-targets --locked
cargo test --locked
cargo clippy --all-targets --all-features --locked -- -D warnings
node --test "tests/node/*.test.js"
```

## Dependencias

Las dependencias se fijan con `Cargo.lock`. GitHub Actions ejecuta los mismos
scripts locales y Dependabot cubre Cargo, npm y workflows.

## Reportes internos

Un reporte debe incluir componente afectado, precondiciones, secuencia de
operaciones, impacto economico, severidad, pruebas recomendadas y mitigacion.
