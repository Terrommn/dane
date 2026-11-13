// Lienzo Excalidraw. Este módulo (y la librería, ~1 MB) se carga solo al abrir un dibujo (React.lazy).
import { Excalidraw } from "@excalidraw/excalidraw";
import "@excalidraw/excalidraw/index.css";
import { useRef } from "react";
import { type Scene, sceneFromExcalidraw } from "./scene";

export default function ExcalidrawCanvas({
  initial,
  onChange,
  dark,
}: {
  initial: Scene;
  onChange: (scene: Scene) => void;
  dark: boolean;
}) {
  const last = useRef<string>("");
  return (
    <div className="h-[70vh] min-h-96 overflow-hidden rounded-xl border border-line">
      <Excalidraw
        langCode="es-ES"
        theme={dark ? "dark" : "light"}
        initialData={{
          elements: (initial.elements ?? []) as never,
          appState: { viewBackgroundColor: initial.appState?.viewBackgroundColor ?? "#ffffff" },
          files: (initial.files ?? {}) as never,
        }}
        onChange={(elements, appState, files) => {
          const scene = sceneFromExcalidraw(
            elements as unknown[],
            appState as unknown as Record<string, unknown>,
            files as unknown as Record<string, unknown>,
          );
          // onChange se dispara también al mover el puntero: solo notificamos cambios reales del contenido.
          const key = JSON.stringify([scene.elements, scene.appState]);
          if (key !== last.current) {
            last.current = key;
            onChange(scene);
          }
        }}
        UIOptions={{
          canvasActions: { loadScene: false, saveToActiveFile: false, export: { saveFileToDisk: true } },
        }}
      />
    </div>
  );
}
