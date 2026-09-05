# Biblioteca de livros — Design

**Spec:** `.specs/features/book-library/spec.md`
**Status:** planejado (2026-09-04). Nenhum arquivo de código foi criado ou alterado.

---

## Pré-condição que não é detalhe

Na sessão em que este design foi escrito, `src/`, `src-tauri/`, `scripts/` e `public/` estavam **apagados no working tree** (124 arquivos em `D` no `git status`), presentes no `HEAD`. Todo este design foi lido contra o `HEAD` (`git show HEAD:<arquivo>`). **A árvore precisa ser restaurada antes da primeira task** — ou a decisão de não restaurá-la precisa ser tomada e registrada, porque nesse caso este design não se aplica.

Além disso: o `AGENTS.md` afirma que `src/types.ts` é gerado desde 2026-07-28 pela feature `generated-types`, e que o frontend tem 63 testes pela `frontend-testing`. **Nenhuma das duas está no `HEAD`** — não há `src-tauri/src/types_export.rs`, e o `package.json` do `HEAD` não tem script `test`. As duas pastas de spec (`generated-types/`, `frontend-testing/`) estão untracked. A T1 confere isso e decide.

---

## A decisão que dirige o resto: tabela nova, não reuso

O caminho óbvio seria pendurar os livros na tabela `documents`, que já existe e já tem import, listagem e remoção prontos. Ele foi medido e descartado:

```rust
// src-tauri/src/document_commands.rs (HEAD)
const DELETE_BORROWED_ROWS: &str = "DELETE FROM documents WHERE namespace <> 'global'";
```

Essa constante roda dentro de `discard_interrupted_attachments`, chamada por `requeue_unfinished_documents`, chamada no setup do app em `src-tauri/src/lib.rs:111`. Ou seja:

- livro gravado com `namespace <> 'global'` → **apagado a cada abertura do app**;
- livro gravado com `namespace = 'global'` → aparece em `SELECT_DOCUMENT`, isto é, na lista de RAG, e é reenfileirado pelo `SELECT_RESUMABLE` se ficar num status não-terminal.

Reusar exigiria alterar as 5 constantes SQL de `document_commands.rs`, o `pipeline`, e os testes que hoje provam justamente esse isolamento. A tabela nova não encosta em nada disso: o diff fica em `db.rs` (uma migração), um arquivo de comandos novo e o registro no `lib.rs`.

---

## Componentes

### Backend

| Arquivo | O que muda |
| --- | --- |
| `src-tauri/src/db.rs` | `MIGRATION_9_BOOKS` + entrada `(9, …)` na lista `MIGRATIONS`. **Conferir na lista que 9 está livre antes de escrever** — a lista termina hoje na 8 (`MIGRATION_8_CHAT_MEMORY`). |
| `src-tauri/src/library_commands.rs` | **novo.** Os 4 comandos, a detecção de DRM e os testes. |
| `src-tauri/src/lib.rs` | `mod library_commands;` + os 4 comandos no `invoke_handler!` + nada no setup (a biblioteca não tem nada para retomar no boot). |

`SUBDIRS` em `config.rs` **não** é alterado: `ensure_folder_structure` só roda no onboarding (`config_commands.rs:46`) e na troca de pasta (`:99`), nunca no boot — acrescentar `"library"` lá não criaria a pasta em nenhuma instalação já existente. Quem garante a pasta é um helper que faz `create_dir_all`, no mesmo padrão que `import_documents` já usa.

#### Esquema (migração 9)

```sql
CREATE TABLE IF NOT EXISTS books (
    id          TEXT PRIMARY KEY,
    filename    TEXT NOT NULL,
    format      TEXT NOT NULL,
    size_bytes  INTEGER NOT NULL,
    imported_at TEXT NOT NULL
);
```

Sem `file_path`: o caminho é sempre `<base_path>/library/<filename>`. Guardar caminho absoluto quebra o modo portátil quando o pen drive muda de letra.

Sem coluna de posição de leitura. Ela entra na migração que vier junto com o leitor, quando existir código que a escreva — ver `reading-history/spec.md`.

#### Comandos

| Comando | Assinatura | Requisitos |
| --- | --- | --- |
| `import_books` | `(paths: Vec<String>) -> ImportBooksResult` | LIB-02..LIB-08 |
| `list_books` | `() -> Vec<BookRecord>` | LIB-09 |
| `delete_book` | `(id: String) -> ()` | LIB-10 |
| `library_path` | `() -> String` | LIB-11, LIB-12 |

`ImportBooksResult` repete a forma que já existe em `ImportResult`: `{ imported: Vec<BookRecord>, rejected: Vec<RejectedImport> }`. Um arquivo ruim não derruba os bons (LIB-03) — é o mesmo contrato de `import_documents`, e o motivo dele existir vale igual aqui.

`library_path` devolve o caminho e a UI usa `openPath()` do `@tauri-apps/plugin-opener` para abrir. Não há comando `open_library_folder`: o frontend precisa do caminho de qualquer forma para LIB-12, então um comando serve aos dois requisitos.

#### Detecção de DRM

Pura, testável, sem I/O de rede:

- **PalmDB (`.mobi`, `.azw`, `.azw3`)** — o cabeçalho PalmDB traz a lista de registros a partir do byte 78, cada entrada com 8 bytes e o offset do registro nos 4 primeiros. No registro 0 (cabeçalho PalmDOC), os bytes 12..14 são o **tipo de criptografia**, `u16` big-endian: `0` = sem DRM, `1` = criptografia antiga, `2` = DRM Mobipocket. Qualquer valor diferente de zero é recusa (LIB-05).
- **EPUB** — é um zip. Presença de `META-INF/encryption.xml` é o sinal de DRM (LIB-06). O crate `zip = "2"` **já está no `Cargo.toml`**; nenhuma dependência nova.
- **PDF** — não verificado nesta feature, por decisão registrada no Out of Scope.

Nenhuma dependência nova entra no projeto por causa desta feature.

### Frontend

| Arquivo | O que muda |
| --- | --- |
| `src/components/Library/LibraryPanel.tsx` | **novo** — substitui o `DocumentsPanel` como conteúdo da aba |
| `src/components/Library/BookRow.tsx` | **novo** — nome, formato, tamanho, botão remover |
| `src/lib/libraryApi.ts` | **novo** — os 4 wrappers de `invoke` |
| `src/store/libraryStore.ts` | **novo** — zustand, no molde do `documentsStore` menos o listener de `document-status` (não há progresso a acompanhar) |
| `src/App.tsx` | `activeView === "documents"` passa a renderizar `LibraryPanel` |
| `src/store/uiStore.ts` | `ActiveView`: `"documents"` → `"library"` |
| `src/components/Sidebar/DocumentsSection.tsx` | vira `LibrarySection.tsx`; a contagem deixa de filtrar por `status === "ready"` (não há status) e passa a ser o total |
| `src/components/Sidebar/Sidebar.tsx` | troca o import da seção |
| `src/i18n/locales/en.json`, `pt.json` | bloco `library.*`; **paridade obrigatória de chaves** |
| `src/types.ts` | `BookRecord`, `ImportBooksResult` — gerado ou à mão, conforme o que a T1 achar |

`DocumentsPanel.tsx`, `DocumentRow.tsx` e `DocumentStatusBadge.tsx` **não são apagados** nesta feature; ficam órfãos de rota e saem na remoção do RAG (AD-052). Apagá-los aqui misturaria a entrega com a revogação.

---

## Fluxo da importação

```
usuário clica importar
  → open() do plugin-dialog, filtro pdf/epub/mobi/azw/azw3        (LIB-01)
  → invoke("import_books", { paths })
      para cada caminho:
        extensão fora da lista?        → rejected                  (LIB-03)
        metadata falhou?               → rejected                  (LIB-05.3)
        tem DRM?                       → rejected, sem copiar      (LIB-05, LIB-06)
        copia para library/, sufixo em colisão                     (LIB-02)
        INSERT INTO books                                          (LIB-07)
      ← { imported, rejected }
  → store recarrega a lista                                        (LIB-09)
```

Não há evento de progresso, não há task assíncrona, não há status. É o que "sem RAG" significa na prática: o comando termina quando o `fs::copy` termina.

---

## O que este design deliberadamente não resolve

- **Reconciliação disco↔banco.** Se o usuário apagar um arquivo pelo explorador, a linha continua. O leitor é quem vai precisar abrir o arquivo e, portanto, quem precisa lidar com a ausência dele.
- **Metadados do livro** (título real, autor, capa). Exige ler o container de cada formato — é trabalho do leitor.
- **Ordenação por leitura recente.** Depende do histórico, que é outra spec.
