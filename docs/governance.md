# Gobierno

## Modelo

`GovernanceRegistry` mantiene gobernadores activos, guardián, política y operaciones. Las aprobaciones se deduplican por `AccountId`; el guardián solo puede cancelar.

## Identidad

```text
domain = governance
subject = chronos-policy-operation-v1

fields = {
  protocol, network, chain_id, target, selector,
  payload_digest, predecessor, salt, eta, expires_at, quorum
}

operation_id = BLAKE3(canonical_envelope(fields))
```

Los campos se ordenan por clave y valor antes del hash. El digest final tiene 32 bytes y se representa con 64 caracteres hexadecimales.

## Política

- `quorum`: número mínimo de gobernadores distintos.
- `min_delay_epochs`: distancia mínima entre schedule y `eta`.
- `max_execution_window_epochs`: anchura máxima entre `eta` y expiración.

El quórum debe caber en el conjunto activo. Delay y ventana máxima son no nulos.

## Ciclo

```mermaid
stateDiagram-v2
    [*] --> PendingApprovals
    PendingApprovals --> Timelocked: quorum
    Timelocked --> BlockedPredecessor: eta y predecesor pendiente
    Timelocked --> Ready: eta y sin dependencia
    BlockedPredecessor --> Ready: predecesor ejecutado
    Ready --> Executed: execute
    PendingApprovals --> Expired: expires_at
    Timelocked --> Expired: expires_at
    BlockedPredecessor --> Expired: expires_at
    Ready --> Expired: expires_at
    PendingApprovals --> Cancelled: guardian
    Timelocked --> Cancelled: guardian
```

La expiración se evalúa antes de quórum y timelock. Una operación expirada no vuelve a estar lista.

## Predecesores

Una operación dependiente solo se habilita si el registro contiene el digest predecesor con estado `Executed`. El identificador liga esa dependencia; sustituirla produce otra operación.

```mermaid
sequenceDiagram
    participant R as Risk
    participant G as Governors
    participant C as Chronos registry
    participant O as Operator
    R->>C: schedule(spec, now)
    C-->>R: operation_id
    G->>C: approve(operation_id)
    G->>C: approve(operation_id)
    O->>C: decision(operation_id, epoch)
    C-->>O: ready / blocked / expired
    O->>C: execute(operation_id, epoch)
    C-->>O: execution receipt
```

## Rotación

El registro no muta el conjunto de gobernadores durante una operación. La integración debe crear un nuevo registro o aplicar una migración gobernada, conservar el conjunto anterior para evidencia y evitar reinterpretar aprobaciones históricas.

## Evidencia

Conservar spec completa, envelope canónico, digest, aprobadores, epoch de schedule, decisión, receipt, payload efectivo y hash del release. El digest BLAKE3 identifica contenido; las firmas externas demuestran identidad.
