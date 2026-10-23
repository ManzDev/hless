# hless

Un visualizador de terminal para archivos HTML con folding anidado por etiquetas, inspirado en `jless` para JSON.

## Instalación

```bash
cargo install hless
```

## Uso

```bash
# Ver un archivo HTML
hless archivo.html

# Ver stdin
cat archivo.html | hless
hless -

# Ayuda y versión
hless            # sin argumentos (y sin stdin por pipe) muestra la ayuda
hless --help
hless --version
```

Si la entrada llega por un pipe (`cat ... | hless`, `curl ... | hless`) pero la
salida es un terminal, el teclado se lee de `/dev/tty`, así que la TUI sigue
siendo interactiva. Si la salida **no** es un terminal, se vuelca el HTML tal
cual (útil para redirigir a un fichero o encadenar con `less`).

## Características

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
  - `/`: Buscar (incremental; `Enter` confirma, `Esc` cancela)
  - `n` / `N`: Siguiente / anterior coincidencia
  - `q`: Salir

## Ejemplo de salida

```
 hless │ test.html                                          HTML · 25 rows
    <!DOCTYPE html>
▶ ▸ <html lang="es"> [...]
 Row 2/25 │ q quit · ↑↓ nav · ←→ fold                             8%
```

La barra superior y la de estado usan fondo a todo el ancho. `▸` marca un nodo
plegado (pulsa `→` para desplegarlo y descubrir el siguiente nivel) y `▾` uno
desplegado; `▶` es la fila con el foco.

## Dependencias

- `html5ever`: Parser HTML
- `termion`: Terminal UI
- `clap`: Línea de comandos

## Comparación con jless

| Característica | jless | hless |
|----------------|-------|-------|
| Formato | JSON | HTML |
| Folding | ✓ | ✓ |
| Colores | ✓ | ✓ |
| Navegación | ✓ | ✓ |
| Búsqueda | ✓ | ✓ |

## Desarrollo

```bash
# Clonar y construir
git clone https://github.com/manz/hless.git
cd hless
cargo build --release

# Instalar
cargo install --path .
```

## License

MIT
