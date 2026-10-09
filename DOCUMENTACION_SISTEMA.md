# 🧟 PROJECT ZOMBOID MOD STUDIO — DOCUMENTACIÓN TÉCNICA Y FUNCIONAL DEL SISTEMA

**Versión del Sistema:** 1.0.0 (Build 41 & Build 42 Ready)  
**Arquitectura:** Frontend React 19 + TypeScript / Backend Tauri 2.0 (Rust) / MCP Server (JSON-RPC 2.0)  
**Repositorio:** `Project Zomboid Mod Studio`

---

## 📌 ÍNDICE GENERAL

1. [Visión General y Objetivos del Proyecto](#1-visión-general-y-objetivos-del-proyecto)
2. [Arquitectura Global del Sistema](#2-arquitectura-global-del-sistema)
3. [Módulo de Perfiles e Instancias (`instance_manager`)](#3-módulo-de-perfiles-e-instancias-instance_manager)
4. [Módulo de Mod List y Orden de Carga (`load_order`)](#4-módulo-de-mod-list-y-orden-de-carga-load_order)
5. [Escáner VFS y Motor de Fusión 3-Way AST (`vfs` y `diff_engine`)](#5-escáner-vfs-y-motor-de-fusión-3-way-ast-vfs-y-diff_engine)
6. [Generador de Parches y Capa de Polyfills B42+ (`patch_generator`)](#6-generador-de-parches-y-capa-de-polyfills-b42-patch_generator)
7. [Monitor Center, Diagnóstico de Crashes y Sandbox (`sandbox`)](#7-monitor-center-diagnóstico-de-crashes-y-sandbox-sandbox)
8. [Live IPC Bridge con el Juego (`Z_PZModStudio_Bridge`)](#8-live-ipc-bridge-con-el-juego-z_pzmodstudio_bridge)
9. [Gestor de Servidores Dedicados (`server_manager`)](#9-gestor-de-servidores-dedicados-server_manager)
10. [Servidor MCP para Agentes de Inteligencia Artificial (`mcp`)](#10-servidor-mcp-para-agentes-de-inteligencia-artificial-mcp)
11. [Flujo de Persistencia y Archivos del Juego](#11-flujo-de-persistencia-y-archivos-del-juego)
12. [Guía de Compilación y Entorno de Desarrollo](#12-guía-de-compilación-y-entorno-de-desarrollo)

---

## 1. VISIÓN GENERAL Y OBJETIVOS DEL PROYECTO

**Project Zomboid Mod Studio** es un entorno integral de desarrollo, gestión, diagnóstico y resolución de conflictos de mods para *Project Zomboid* (compatible tanto con **Build 41** como con **Build 42+**).

### Objetivos Principales:
- **Gestión de Perfiles Aislados:** Crear combinaciones independientes de mods (ej: *Vanilla Plus*, *Hardcore Survival*, *Servidor Co-op*) y alternar entre ellas en 1 clic sin perder el orden ni configuraciones.
- **Resolución de Conflictos VFS (Virtual File System):** Detectar colisiones donde dos o más mods sobrescriben el mismo archivo de script o código Lua, ofreciendo fusión automática mediante AST (Abstract Syntax Tree) o manual asistida.
- **Capa de Polyfills Build 42+:** Inyectar automáticamente capas de compatibilidad para tablas y métodos deprecados o modificados en la transición de B41 a B42.
- **Diagnóstico y Telemetría en Tiempo Real:** Capturar y traducir excepciones de Java/Lua desde `console.txt` y ofrecer soluciones automatizadas en tarjetas explicativas.
- **Control del Juego y Bridge en Vivo:** Lanzar el juego en modos de depuración (`-debug`, ventana, sin steam) y comunicarse bidireccionalmente con la sesión mediante un mod de enlace.
- **Integración con Agentes de IA vía MCP:** Servidor Model Context Protocol nativo en Rust para permitir que asistentes autónomos (Antigravity, Claude, Cursor, Windsurf, Roo Code) diagnostiquen, organicen y fusionen scripts.

---

## 2. ARQUITECTURA GLOBAL DEL SISTEMA

El proyecto utiliza una arquitectura desacoplada y de alto rendimiento:

```
┌──────────────────────────────────────────────────────────────────┐
│                   FRONTEND (React 19 + TypeScript)               │
│  - Tailwind CSS + Lucide Icons + Monaco Editor                   │
│  - Módulos: Profiles, Mod List, Merger, Monitor, Servers, Settings│
└─────────────────────────────────┬────────────────────────────────┘
                                  │ IPC (Tauri Core / Events)
┌─────────────────────────────────▼────────────────────────────────┐
│                   BACKEND TAURI 2.0 (Rust)                       │
│  ├── vfs: Resolución de rutas y detección de colisiones VFS      │
│  ├── diff_engine: AST Parser Lua (full_moon) + PZ Data Parser    │
│  ├── load_order: Parser mod.info + Ordenamiento Topológico       │
│  ├── instance_manager: Persistencia JSON de perfiles             │
│  ├── patch_generator: Paquetes sintéticos Z_PZModStudio_*        │
│  ├── sandbox: Proceso ProjectZomboid64.exe + Watcher console.txt │
│  └── server_manager: Control StartServer64.bat + Sync INI        │
└─────────────────────────────────┬────────────────────────────────┘
                                  │ JSON-RPC 2.0 sobre stdio
┌─────────────────────────────────▼────────────────────────────────┐
│              MCP SERVER STANDALONE (pz-mcp-server)               │
│  - 22 Tools + 4 Resources accesibles para Agentes de IA          │
└──────────────────────────────────────────────────────────────────┘
```

---

## 3. MÓDULO DE PERFILES E INSTANCIAS (`instance_manager`)

Ubicación del código:
- Backend: `src-tauri/src/instance_manager/mod.rs`
- Frontend: `src/components/instances/InstanceModule.tsx`

### Funcionamiento:
1. **Almacenamiento Aislado:**  
   Los perfiles se guardan como archivos JSON independientes en:  
   `C:\Users\<Usuario>\Zomboid\PZModStudio_Instances\inst_<id>.json`
   
   **Estructura del Perfil (`AppInstance`):**
   ```json
   {
     "id": "inst_1787853592",
     "name": "Hardcore B42",
     "description": "Mods de supervivencia extrema y armas",
     "created_at": "1787853592",
     "is_active": true,
     "active_mod_ids": ["Brita", "Arsenal(26)GunFighter[MAIN MOD 2.0]", "modoptions"],
     "load_order": ["modoptions", "Arsenal(26)GunFighter[MAIN MOD 2.0]", "Brita"]
   }
   ```

2. **Activación en 1 Clic (`activate_instance`):**  
   Al activar un perfil, el backend de Rust escribe simultáneamente los siguientes archivos del juego:
   - `Zomboid/mods.txt`: Formato de lista plana utilizado por **Project Zomboid Build 42**.
   - `Zomboid/mods/default.txt`: Formato de bloques `mods { mod = ID, }` utilizado por **Build 41**.
   - `Zomboid/ModListData.ini`: Lista de mods activos leída por los lanzadores y el juego base.
   - `Zomboid/PZModStudio_MasterLoadOrder.json`: Registro maestro de orden absoluto.

---

## 4. MÓDULO DE MOD LIST Y ORDEN DE CARGA (`load_order`)

Ubicación del código:
- Backend: `src-tauri/src/load_order/mod_info.rs`, `topological_sort.rs`, `ini_parser.rs`
- Frontend: `src/components/load_order/LoadOrderModule.tsx`

### Funcionamiento:
1. **Escaneo Recursivo y Robusto:**  
   Escanea tanto la carpeta de Steam Workshop (`steamapps/workshop/content/108600`) como la carpeta de mods locales (`Zomboid/mods`).
   - Lee archivos `mod.info` con recuperación automática de codificación (UTF-8, UTF-8 con BOM, Latin-1 y Windows-1252 para evitar caracteres corruptos como `Ã±`).
   - Soporta la estructura multi-versión nativa de B42 (`42/mod.info`, `42.20/`, `common/`).

2. **Ordenamiento Topológico de Dependencias (Algoritmo de Tarjan):**  
   Ordena la lista de mods respetando estrictamente:
   - `require=`: Mods obligatorios que deben cargarse antes.
   - `loadModAfter=`: Mods recomendados para preceder la carga.
   - `incompatible=`: Detección de incompatibilidades directas.
   - Detecta y previene dependencias circulares (bucles infinitos de carga).

3. **Heurísticas Integradas:**  
   Reglas preconfiguradas para paquetes populares que omiten dependencias explícitas en sus manifiestos (ej: *Brita's Weapon Pack* -> *Arsenal GunFighter* -> *ModOptions*; *Tsar's Common Library* para vehículos).

---

## 5. ESCÁNER VFS Y MOTOR DE FUSIÓN 3-WAY AST (`vfs` y `diff_engine`)

Ubicación del código:
- Backend: `src-tauri/src/vfs/mod.rs`, `src-tauri/src/diff_engine/lua.rs`, `src-tauri/src/diff_engine/pz_scripts.rs`
- Frontend: `src/components/merger/MergerModule.tsx`

### Funcionamiento:
1. **Detección de Colisiones VFS:**  
   En Project Zomboid, si dos mods contienen la misma ruta relativa (ej: `media/lua/shared/Translate/ItemName_ES.txt` o `media/lua/client/ISUI/ISInventoryPane.lua`), el último mod en el orden de carga sobreescribe totalmente al primero, rompiendo funciones del mod anterior.
   - El escáner detecta estas colisiones comparando cada archivo competidor con la versión vanilla original del juego base.

2. **Parser y Fusión AST de Lua (`full_moon`):**  
   - Analiza el Árbol de Sintaxis Abstracta (AST) de cada script.
   - Identifica funciones añadidas, tablas extendidas y variables modificadas.
   - Combina automáticamente los cambios de ambos mods en un solo script unificado sin sobrescritura destructiva.

3. **Fusión de Scripts Declarativos de PZ (`.txt`):**  
   - Fusiona definiciones de `item`, `craftRecipe`, `recipe`, `sound`, deduplicando propiedades y combinando tags semánticos.

---

## 6. GENERADOR DE PARCHES Y CAPA DE POLYFILLS B42+ (`patch_generator`)

Ubicación del código:
- Backend: `src-tauri/src/patch_generator/mod.rs`
- Frontend: `src/components/polyfills/PolyfillsModule.tsx`

### Funcionamiento:
1. **Creación de Paquetes Sintéticos (`Z_PZModStudio_*`):**  
   Al presionar *Auto-Merge* o *Package*, el sistema genera un mod real en disco bajo:  
   `Zomboid/mods/Z_PZModStudio_MergedPatch/`  
   (o el nombre del paquete personalizado).
2. **Inyección de Polyfills Lua (`Z_PZModStudio_Polyfills.lua`):**  
   Crea tablas y funciones puente para emular APIs de Build 41 que fueron cambiadas en Build 42, permitiendo que mods clásicos funcionen sin crashear el motor Kahlua:
   - Polyfill de tablas globales de fluidos y recetas.
   - Shim de llamadas de `ItemPickerJava`.
   - Compatibilidad de indexación 0-based de Java vs 1-based de Lua.

---

## 7. MONITOR CENTER, DIAGNÓSTICO DE CRASHES Y SANDBOX (`sandbox`)

Ubicación del código:
- Backend: `src-tauri/src/sandbox/mod.rs`
- Frontend: `src/components/sandbox/SandboxModule.tsx`

### Funcionamiento:
1. **Control de Proceso (`ProjectZomboid64.exe`):**  
   Lanza el juego directamente con flags configurables:
   - `-debug`: Habilita la consola de depuración y menú de desarrollador de PZ.
   - `-windowed`: Ejecución en ventana para monitorear logs al lado de la app.
   - `-nosteam`: Modo local sin autenticación obligatoria de Steam.
2. **Log Watcher de `console.txt` en Tiempo Real:**  
   Supervisa las líneas escritas por la JVM en `C:\Users\<Usuario>\Zomboid\console.txt`.
3. **Taxonomía de Excepciones y Tarjetas de Solución:**  
   Cuando ocurre un error (ej. `KahluaThreadException`, `NoSuchMethodError`, `NullPointerException`), el sistema:
   - Identifica el mod causante y el archivo exacto.
   - Genera una **Tarjeta de Diagnóstico** explicando la causa en lenguaje claro.
   - Provee un botón de **Aplicar Solución / Polyfill** en 1 clic.

---

## 8. LIVE IPC BRIDGE CON EL JUEGO (`Z_PZModStudio_Bridge`)

Ubicación del código:
- Backend: `src-tauri/src/sandbox/mod.rs` (funciones IPC)
- Mod acompañante: `mods/Z_PZModStudio_Bridge/`

### Funcionamiento:
1. **Instalación Automática:** Se instala en `Zomboid/mods/Z_PZModStudio_Bridge`.
2. **Protocolo de Comunicación Basado en Archivos:**  
   - La aplicación escribe comandos JSON en `Zomboid/Lua/pz_mod_studio_command.json`.
   - El mod escucha en el hook `OnTick` / `OnGameStart` dentro de Project Zomboid.
   - Ejecuta la acción (`eval_lua`, `give_item`, `set_godmode`, `reload_lua_file`) y escribe la respuesta en `pz_mod_studio_response.json`.
   - Permite depuración en caliente sin necesidad de reiniciar el juego.

---

## 9. GESTOR DE SERVIDORES DEDICADOS (`server_manager`)

Ubicación del código:
- Backend: `src-tauri/src/server_manager/mod.rs`
- Frontend: `src/components/server/ServerModule.tsx`

### Funcionamiento:
1. **Lanzamiento de `StartServer64.bat`:** Control de inicio y parada de servidores locales.
2. **Edición y Sincronización de `servertest.ini`:**  
   Permite sincronizar con 1 solo clic los mods y Workshop IDs activos del perfil del cliente hacia la configuración del servidor dedicado (`Mods=` y `WorkshopItems=`).
3. **Monitoreo de Jugadores y Comandos:** Lectura de logs de conexión y envío de comandos administrativos.

---

## 10. SERVIDOR MCP PARA AGENTES DE INTELIGENCIA ARTIFICIAL (`mcp`)

Ubicación del código:
- Backend: `src-tauri/src/mcp/` (protocol.rs, server.rs, tools.rs)
- Binario ejecutable: `pz-mcp-server.exe`

### Catálogo de 22 Herramientas (Tools):
- `get_game_status`: Consulta estado y PID de `ProjectZomboid64.exe`.
- `launch_game`: Ejecuta el juego con argumentos de debug y modo ventana.
- `terminate_game`: Cierra o fuerza el cierre del juego.
- `send_game_ipc_command`: Envía comando al mod de enlace en vivo.
- `get_game_ipc_response`: Lee la última respuesta del mod de enlace.
- `install_bridge_companion_mod`: Instala el mod acompañante en `Zomboid/mods`.
- `get_monitor_logs`: Lee y filtra líneas recientes de `console.txt`.
- `list_available_logs`: Lista todos los logs en `Zomboid/Logs/`.
- `read_log_file`: Lee un log específico con filtrado de excepciones.
- `get_crash_diagnostics`: Genera tarjetas de diagnóstico B41/B42 con sugerencias.
- `get_studio_paths`: Retorna las rutas de instalación y carpetas de usuario.
- `list_installed_mods`: Lista todos los mods instalados locales y de Workshop.
- `sort_mod_load_order`: Ejecuta ordenamiento topológico y reporta dependencias.
- `scan_mod_conflicts`: Escanea el VFS y detecta colisiones de archivos.
- `scan_mod_diagnostics`: Ejecuta el motor de conflictos y devuelve diagnósticos en lenguaje claro (dependencias faltantes, pares incompatibles, ciclos, duplicados, versionMin inválidas).
- `list_mod_profiles`: Lista todos los perfiles de mods guardados.
- `create_mod_profile`: Crea un nuevo perfil con orden de carga independiente.
- `activate_mod_profile`: Activa un perfil y lo escribe en el juego.
- `validate_lua_syntax`: Valida sintaxis Lua con línea y columna exacta.
- `merge_lua_scripts`: Realiza fusión 3-Way AST de scripts en conflicto.
- `list_merged_packages`: Lista paquetes de parche registrados.
- `get_master_patch_status`: Consulta el estado del parche maestro.
- `save_draft_resolution`: Guarda código resuelto en el paquete de parche.

### Catálogo de 4 Recursos (Resources):
- `pz://monitor/console-log`: Flujo en tiempo real de `console.txt`.
- `pz://mods/installed-summary`: Resumen en JSON de mods instalados y orden.
- `pz://paths/config`: Configuración actual de rutas del sistema.
- `pz://patches/status`: Estado del paquete maestro de parches.

---

## 11. FLUJO DE PERSISTENCIA Y ARCHIVOS DEL JUEGO

```
C:\Users\<Usuario>\Zomboid\
├── console.txt                          <-- Log activo del motor (monitoreado)
├── mods.txt                             <-- Mods activos (PZ Build 42)
├── ModListData.ini                      <-- Mods activos y orden (Universal)
├── PZModStudio_MasterLoadOrder.json     <-- Orden maestro persistente de la app
├── PZModStudio_Instances/               <-- Carpeta de perfiles de la app
│   ├── inst_1787853592.json
│   └── inst_coop_server.json
└── mods/
    ├── default.txt                      <-- Mods activos (PZ Build 41)
    ├── Z_PZModStudio_Bridge/            <-- Mod acompañante IPC en vivo
    └── Z_PZModStudio_MergedPatch/       <-- Parche maestro generado por la app
        ├── mod.info
        ├── 42/mod.info
        ├── patch_metadata.json
        └── media/lua/shared/
            └── Z_PZModStudio_Polyfills.lua
```

---

## 12. GUÍA DE COMPILACIÓN Y ENTORNO DE DESARROLLO

### Requisitos Previos:
- **Node.js:** Versión 18+ o 20+ LTS (`node`, `npm`).
- **Rust Toolchain:** Stable x86_64-pc-windows-msvc (`rustup`, `cargo`).
- **Visual Studio C++ Build Tools:** Con componentes de C++ para Windows.

### Comandos de Ejecución:

```powershell
# 1. Instalar dependencias de Node
npm install

# 2. Ejecutar la aplicación en modo desarrollo (Hot-Reload):
npm run tauri dev

# 3. Compilar el servidor MCP standalone (Rápido, sin interfaz gráfica):
cargo build --release --bin pz-mcp-server --manifest-path "src-tauri/Cargo.toml"

# 4. Compilar la aplicación completa en binario portable (.exe):
npm run build
npm run tauri build
```
