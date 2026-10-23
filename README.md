# hless

Un visualizador de código HTML vía terminal, con soporte de folding (anidado) por etiquetas e inspirado en `jless` para JSON.

![hless: interactive HTML viewer](hless.gif)

## Instalación

```bash
cargo install --git https://github.com/manzdev/hless

hless --version
```

## Uso

```bash
# Ver archivo HTML
hless archivo.html
cat archivo.html | hless

# Acceder a una web y visualizar su código
curl -I https://manz.dev/ | hless
curl -i https://manz.dev/ | hless    # Ctrl+O muestra las cabeceras

# Forzar contenido que no parece HTML
hless --force notas.txt
```

En principio, `hless` comprueba que la entrada parece HTML. Si no, muestra un error. Usa `--force` si quieres obligarlo a leer el código.

## Características

- **Detección de HTML**: valida que la entrada sea HTML antes de abrir la TUI
- **Cabeceras HTTP**: si vienes de `curl -i`/`-I`, se parsean y se muestran en un popup (`Ctrl+O`)
- **Folding anidado**: Colapsa y expande etiquetas HTML anidadas
- **Indentación por nivel**: Muestra la estructura del DOM con indentación visual
- **Resaltado de colores**: Diferentes colores para etiquetas, atributos y texto
- **Búsqueda incremental**: `/` para buscar texto/etiquetas/atributos, con resaltado y `n`/`N`
- **Navegación con teclado**:
  - `↑` / `↓` (o `k` / `j`): Navegar entre elementos
  - `←` / `→` (o `h` / `l`): Colapsar/expandir elementos
  - `Space` / `f` / `PageDown`: Avanzar una página (el cursor mantiene su línea)
  - `b` / `PageUp`: Retroceder una página
  - `g` / `G`: Ir al inicio/final
  - `Ctrl+O`: Mostrar/ocultar las cabeceras HTTP (si las hay). Dentro del popup: `↑↓`/`j`/`k` scroll vertical, `←→`/`h`/`l` scroll horizontal, `Esc` cierra
  - `/`: Buscar (incremental; `Enter` confirma, `Esc` cancela)
  - `n` / `N`: Siguiente / anterior coincidencia
  - `q`: Salir

## Dependencias

- `html5ever`: Parser HTML
- `termion`: Terminal UI
- `clap`: Línea de comandos

## Comparación con jless

| Característica | jless | hless |
|----------------|-------|-------|
| Formato | JSON | HTML |
| Folding | ✅ | ✅ |
| Colores | ✅ | ✅ |
| Navegación | ✅ | ✅ |
| Búsqueda | ✅ | ✅ |

## Compilar

```bash
git clone https://github.com/manzdev/hless.git
cd hless
cargo build --release
```

## License

MIT
