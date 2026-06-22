# Cobblemon Studio

Herramienta de escritorio **no-code** para crear y previsualizar contenido de [Cobblemon](https://cobblemon.com/): datapacks + resource packs (Pokémon nuevos, reskins/aspectos, formas regionales, cosméticos, evoluciones, apariciones, Pokédex, monturas, objetos, recetas…).

- 🧩 **Asistentes guiados** para cada tipo de contenido (modo Fácil) y editor por bloques (modo Experto).
- 🧊 **Visor 3D integrado** (modelos Bedrock `.geo.json` + texturas + animación idle).
- 📦 **Inspector del pack** estilo IDE: explora, previsualiza y edita los archivos de tu pack.
- 🆕 **Crear Pokémon** y **Reskin** con importación de tus propios modelos.
- ⬇️ Exporta datapack + resource pack listos para Cobblemon 1.7.x.
- 🔄 **Auto-actualización**: comprueba si hay versión nueva al abrir.

Construida con [Tauri](https://tauri.app/) (binario nativo ligero, ~6 MB de instalador).

## Descargar

Descarga el instalador más reciente desde [**Releases**](../../releases/latest).

### ⚠️ Aviso de Windows (SmartScreen) al instalar

Al abrir el instalador, Windows mostrará **"Windows protegió su PC" / "Editor desconocido"**. Es **normal y esperado**: la app es nueva y todavía no está firmada con un certificado de pago; no significa que sea peligrosa (el código es público en este repositorio).

Para instalar igualmente:

1. En la ventana azul *"Windows protegió su PC"* → pulsa **Más información**.
2. Aparecerá el botón **Ejecutar de todas formas** → púlsalo.
3. (En el navegador, si bloquea la descarga: menú **···** del archivo → **Conservar** → **Conservar de todas formas**.)

El aviso se irá reduciendo a medida que más gente la descargue. Más adelante el instalador podría firmarse para que no aparezca.

## Desarrollo

```bash
npm install
npm run tauri dev      # desarrollo
npm run tauri build    # instalador (NSIS)
```

## Licencia

Proyecto independiente, no afiliado al equipo de Cobblemon. Cobblemon es marca de sus respectivos autores.
