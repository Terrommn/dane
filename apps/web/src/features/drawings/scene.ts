// Escena persistida de un dibujo: solo lo necesario para reconstruirlo (sin estado efímero de la UI).

export interface Scene {
  elements: unknown[];
  appState?: { viewBackgroundColor?: string };
  files?: Record<string, unknown>;
}

/** Límite aproximado aceptado por el servidor (~1.9 MB de JSON). */
export const MAX_SCENE_BYTES = 1_900_000;

export function sceneFromExcalidraw(
  elements: unknown[],
  appState: Record<string, unknown>,
  files: Record<string, unknown>,
): Scene {
  const alive = elements.filter((e) => !(e as { isDeleted?: boolean }).isDeleted);
  const usedFiles = new Set(alive.map((e) => (e as { fileId?: string }).fileId).filter(Boolean));
  const keptFiles: Record<string, unknown> = {};
  for (const [id, f] of Object.entries(files ?? {})) if (usedFiles.has(id)) keptFiles[id] = f;
  return {
    elements: alive,
    appState: { viewBackgroundColor: (appState.viewBackgroundColor as string | undefined) ?? "#ffffff" },
    files: keptFiles,
  };
}

export function sceneSize(scene: Scene): number {
  return new TextEncoder().encode(JSON.stringify(scene)).length;
}

export function asScene(v: unknown): Scene {
  const o = (v ?? {}) as Partial<Scene>;
  return {
    elements: Array.isArray(o.elements) ? o.elements : [],
    appState: o.appState ?? {},
    files: o.files ?? {},
  };
}
