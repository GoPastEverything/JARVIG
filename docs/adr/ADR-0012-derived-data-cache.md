# ADR-0012 — Content-addressed derived data

Status: Accepted
Date: 2026-09-22

## Context

Baked meshes, textures, shaders, and collision must be reproducible. A timestamp cache hides which input changed.

## Decision

A derived key is SHA-256 over source bytes, import settings, importer version, engine format version, target platform, and feature flags. Canonical JSON makes object key order irrelevant. Missing values throw instead of being omitted. Cooked products are not committed except golden fixtures. The disk cache directory is gitignored.

## Alternatives Considered

- Last-modified timestamps. Rejected. They do not capture tool or flag changes.
- A single global cache key per file path. Rejected. Settings and platform would alias.

## Consequences

Importers and compilers must version themselves. A silent behavior change without a version bump is a cache bug.

## Supersedes

Nothing.

## Superseded By

Nothing.
