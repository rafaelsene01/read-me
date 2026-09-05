# Validação independente — `book-library`

**Verificador:** agente independente (não escreveu nenhuma linha desta feature).
**Data:** 2026-09-05. **Árvore:** `master`, working tree sujo (feature não commitada).
**Revisão 2 (2026-09-05, mesma sessão):** reavaliação após outro agente corrigir o **D-01** e criar o marcador `SPEC:` de `document_commands.rs`. As duas mudanças foram medidas por mim na árvore, não aceitas por relato. Ver §4 e §7.
**Método:** gates executados pelo próprio verificador + leitura dos arquivos. **O app não foi aberto** e nenhum `invoke` foi disparado — por decisão do escopo desta validação (a UAT é a T9, do usuário).

---

## 1. Veredito por requisito

| ID | Veredito | Evidência (`arquivo:linha`) | O que falta |
| --- | --- | --- | --- |
| **LIB-01** | `IMPLEMENTED, NOT EXERCISED` | `src/components/Library/LibraryPanel.tsx:12` (`BOOK_EXTENSIONS = ["pdf","epub","mobi","azw","azw3"]`) e `:35-39` (`open({ multiple: true, filters: [{ extensions: BOOK_EXTENSIONS }] })`). Compilado: `npm run build` exit 0 (§2). | O seletor é janela do SO. Só abrindo o app. **T9.** |
| **LIB-02** | `VERIFIED` | Teste executado: `src-tauri/src/library_commands.rs:500-537` (`a_second_book_with_the_same_name_gets_a_suffix_instead_of_overwriting`) — asserta `livro.pdf` == `b"primeiro"` (`:526`), `livro (2).pdf` == `b"segundo"` (`:527`) e que a **linha do banco** grava o nome de destino (`:531-536`). Código: `:188` (`unique_destination`) e `:201-204`. Helper reusado, não reescrito: `src-tauri/src/document_commands.rs:37`. | Nada para o critério em si. O comando `import_books` (`:273-286`), que resolve pasta e conexão, nunca rodou. |
| **LIB-03** | `VERIFIED` (backend) / `IMPLEMENTED, NOT EXERCISED` (UI) — **D-01 corrigido, ver §4** | Testes executados: `library_commands.rs:346-351`, `:353-360` (aceita os 5, recusa `.docx`/`.kfx`/`.txt`/`.md`/sem extensão) e `:569-607` (`a_mixed_selection_…`: 1 importado, 2 recusados nomeados, e `:603-605` prova que os recusados **não tocam a pasta**). Código: `:21-23`, `:156-159`. UI: `LibraryPanel.tsx:89-96`, com o basename agora em `:92` (`split(/[\\/]/)`). | A lista de recusa **nunca foi vista na tela**. A correção do D-01 está provada **por leitura do regex**, não por execução: não há suíte de frontend e o app não foi aberto. |
| **LIB-04** | `IMPLEMENTED, NOT EXERCISED` | `library_commands.rs:121-128` (`library_dir` → `load_config(app)?.ok_or_else(…"Nenhuma pasta de armazenamento configurada ainda")`), chamada antes de qualquer cópia em `:279`. UI mostra o erro em `LibraryPanel.tsx:86`. | `library_dir()` exige `AppHandle` e **não é chamada por nenhum teste** (confirmado: `import_all` recebe `dir: &Path` já resolvido, `:143`). Zero execução. **T9.** |
| **LIB-05** | `VERIFIED` (sintético) | Testes executados: `library_commands.rs:362-367` (campo 0 → `Ok(false)`), `:369-377` (campos 1 e 2 → `Ok(true)`), `:379-406` (truncado, offset 0, offset além do EOF e arquivo inexistente → **todos `Err`**). Código: `:53-81`, guarda `record0 < 78` em `:66-72`. Recusa **antes** da cópia: `:172-182` vem antes do `fs::copy` em `:189`. | Nenhum `.mobi`/`.azw`/`.azw3` **real** passou por aqui — os arquivos são montados byte a byte em `:323-333`. Os offsets batem com o formato documentado, não com um arquivo de Kindle. |
| **LIB-06** | `VERIFIED` (sintético) | Testes executados: `library_commands.rs:408-413` (com `META-INF/encryption.xml` → `Ok(true)`), `:415-420` (sem → `Ok(false)`), `:422-427` (não-zip → `Err`). Código: `:85-94`. | Nenhum EPUB real, com ou sem DRM. |
| **LIB-07** | `VERIFIED` (parcial — ver ressalva) | Migração: `src-tauri/src/db.rs:171` (`MIGRATION_9_BOOKS`), registrada em `:194`; testes executados `db.rs:514` (`books_is_migration_nine`, compara contra a **lista**, não a doc), `:523` (banco novo → `user_version = 9` + as 5 colunas na ordem) e `:544` (banco parado no 8 sobe pro 9 preservando `chats`/`messages`/`documents` e criando `books` vazia). Import: `library_commands.rs:539-567`, `COUNT(*) FROM books = 1` em `:558-561`. Ausência de RAG por leitura: `import_all` (`:143-233`) importa só `unique_destination`, `RejectedImport` e `extension_of` — **nenhum símbolo de `rag::pipeline` ou `rag::store`** (`:4-14`). | A parte "SHALL NOT executar extração/chunking/embedding/LanceDB" é provada **por ausência de chamada lida no código**, não por asserção. E a migração **não foi ensaiada contra cópia de banco real** (`db::real_database` continua `#[ignore]`, entre os 15 ignorados). |
| **LIB-08** | `VERIFIED` | Teste executado `library_commands.rs:539-567`; **li o corpo**: `:563-566` faz `SELECT COUNT(*) FROM documents` e asserta `0` **depois** de `import_all` ter retornado 1 importado (`:550-551`). Não passa por motivo errado: a tabela `documents` **existe** no banco migrado (`db.rs:544-566` insere nela num teste irmão), então o `query_row` não está mascarando "tabela ausente"; e o import é comprovadamente bem-sucedido pela asserção `books = 1` em `:561`. | Nada. É o critério mais bem provado da feature. |
| **LIB-09** | `VERIFIED` (backend) / `IMPLEMENTED, NOT EXERCISED` (UI) | Teste executado `library_commands.rs:453-469` (`books_are_listed_from_the_newest_to_the_oldest`, timestamps explícitos, `ids == ["novo","meio","velho"]`). SQL: `:236-240` (`ORDER BY imported_at DESC`). Store não reordena: `src/store/libraryStore.ts:36-38`. Nome/formato/tamanho na tela: `src/components/Library/BookRow.tsx:26-29`. | A lista **nunca foi renderizada**: não há suíte de frontend nesta árvore (§2, gate 4). **T9.** |
| **LIB-10** | `VERIFIED` (backend) / `IMPLEMENTED, NOT EXERCISED` (UI) | Testes executados `library_commands.rs:484-498` (arquivo some do disco) e `:471-482` (arquivo já ausente → `Ok(())` e a linha some mesmo assim). Código `:253-271`. Botão: `BookRow.tsx:31-37` → `LibraryPanel.tsx:103` → `libraryStore.ts:67-74`. | O botão nunca foi clicado; `delete_book` (`:294-299`) nunca rodou. |
| **LIB-11** | `IMPLEMENTED, NOT EXERCISED` | Escrito: `library_commands.rs:126` (`create_dir_all` dentro de `library_dir`, único caminho de acesso à pasta — o que faz **LIB-11.3** valer também para biblioteca vazia); `:304-307` (`library_path`); `LibraryPanel.tsx:68` (`openPath(libraryPath)`), desabilitado enquanto `null` (`:69`). **LIB-11.4** (portátil): `:125` usa `cfg.base_path_buf().join("library")` e `src-tauri/src/config.rs:45-47` devolve o `base_path` cru — sob AD-034 isso é `./data`. Confirmei que `library` **não** está em `config.rs:126` (`SUBDIRS` tem 5: models, documents, vectors, chats, runtime), o que sustenta o comentário `:113-117`. | **Zero execução.** `library_dir()` nunca rodou; `openPath` nunca foi chamado; o modo portátil nunca foi montado. **T9.** |
| **LIB-12** | `IMPLEMENTED, NOT EXERCISED` | `libraryStore.ts:12` (`libraryPath: string \| null`) e `:44-50` (`loadLibraryPath`); render **ao lado** dos botões, não atrás de clique: `LibraryPanel.tsx:76-80`, carregado no mount em `:29-32`. | A tela nunca montou. Não se sabe se o caminho aparece nem se o `truncate` (`:77`) o deixa legível. **T9.** |

**Contagem exata (12 IDs) — revisão 2, depois da correção do D-01:**

- **`VERIFIED`: 8** — LIB-02, LIB-03, LIB-05, LIB-06, LIB-07, LIB-08, LIB-09, LIB-10. Em LIB-03, LIB-09 e LIB-10 o `VERIFIED` vale **só para a camada de backend/SQL**; a UI dos três continua sem nenhum exercício.
- **`IMPLEMENTED, NOT EXERCISED`: 4** — LIB-01, LIB-04, LIB-11, LIB-12.
- **`DEFECT`: 0** (era 1; o D-01 foi corrigido — §4).
- **`NOT VERIFIED`: 0.**

Na revisão 1 a contagem era 7 / 4 / 1 / 0.

---

## 2. Gates executados por mim

### Gate 1 — `cd src-tauri && cargo test --lib`

```
test result: ok. 195 passed; 0 failed; 15 ignored; 0 measured; 0 filtered out; finished in 7.06s
EXIT=0
```

**195 / 0 / 15 — bate exatamente com o alegado.** Sem divergência.

**Repetido na revisão 2**, depois de o marcador `SPEC:` ser adicionado a `document_commands.rs`: `195 passed; 0 failed; 15 ignored` de novo. Nessa rodada apareceu **uma** linha de `warning`, e ela **não é um warning de código Rust**:

```
warning: linker stdout: Criando biblioteca …\tauri_app_lib-….lib e objeto …\tauri_app_lib-….exp
warning: `tauri-app` (lib test) generated 1 warning
```

É o aviso informativo do linker MSVC ao gerar a import library do binário de teste — não existe no `cargo check --lib`, que é o gate onde "zero warnings" foi alegado e onde eu medi zero (gate 2). Registro para ninguém depois ler isso como uma regressão.

### Gate 2 — `cd src-tauri && cargo check --lib`

Rodado **duas vezes**: a primeira aproveitou cache e só imprimiu `Finished`. Como cache não prova ausência de warning, forcei recompilação (`touch src/lib.rs src/library_commands.rs src/db.rs`) e filtrei:

```
$ cargo check --lib 2>&1 | grep -iE "^(warning|error)" | sort | uniq -c
(saída vazia)
EXIT=0
```

**Zero warnings — bate com o alegado.** A segunda medição é a que vale; a primeira teria sido a armadilha da AD-041 (passar por motivo errado).

### Gate 3 — `npm run build`

```
✓ 1859 modules transformed.
dist/assets/index-CN6mYczN.css   20.33 kB │ gzip:  4.90 kB
dist/assets/index-BhmqRmEJ.js   315.80 kB │ gzip: 96.16 kB
✓ built in 2.66s
EXIT=0
```

**Exit 0 e bundle `index-BhmqRmEJ.js` — bate exatamente com o alegado.**

**Repetido na revisão 2**, depois da correção do D-01 em `LibraryPanel.tsx:92`: exit 0, 1859 módulos, `dist/assets/index-C5uO1JD5.js` 315.80 kB. **O hash do bundle mudou de propósito** — o fonte mudou. Quem for conferir o alegado antigo (`index-BhmqRmEJ.js`) vai achar um hash diferente e isso está certo; o hash de referência a partir daqui é `index-C5uO1JD5.js`.

### Gate 4 — `npm test` (**divergência com o alegado**)

Alegado: "não aplicável". **Medido:** o script existe (`package.json:10` → `vitest run`), o `vitest` **está instalado** (`node_modules/.bin/vitest`, `vitest@^4.1.10` nas devDependencies) e o comando **roda e falha**:

```
RUN  v4.1.10 D:/read-me
No test files found, exiting with code 1
include: src/**/*.test.ts, src/**/*.test.tsx
```

**Exit code 1**, não "não aplicável". Confirmado o resto da alegação: `find src -name "*.test.ts*"` → **zero arquivos**; `src/test/` **não existe** (logo `setupFiles: ["./src/test/setup.ts"]` e os dois dobles de `vitest.config.ts:6` e `:19-22` apontam para nada). Isso **não** foi causado por esta feature — é a árvore que não tem a feature `frontend-testing` commitada — mas **contradiz o `AGENTS.md`**, que afirma `npm test` em "63 passando em 8 arquivos". Registro a contradição: **o que eu medi vence**.

### Gate 5 — paridade i18n (medido por mim, achatando as chaves aninhadas)

```
en keys: 158   pt keys: 158
só em en: []   só em pt: []
```

**158/158, zero divergência — bate com o alegado.** Conferi também, uma a uma, as **11 chaves que os componentes chamam**, e todas existem nos **dois** arquivos:

`library.title`, `library.import`, `library.importing`, `library.fileDialogTitle`, `library.supportedFormats`, `library.openFolder`, `library.rejected`, `library.empty`, `library.remove`, `sidebar.library`, `settings.back` → **EN ok / PT ok** em todas. Nenhuma chave chamada e ausente (que apareceria como texto cru e nenhum gate pegaria).

### Gate 6 — dependências novas

```
$ git diff src-tauri/Cargo.toml package.json src-tauri/Cargo.lock package-lock.json
(diff vazio)
```

**Nenhuma dependência nova — confirmado.** O crate `zip`, usado pela detecção de DRM em EPUB, **já existia**: `src-tauri/Cargo.toml:38` (`zip = "2"`).

### Gate 7 — `src-tauri/src/types_export.rs`

```
$ ls src-tauri/src/types_export.rs
No such file or directory
$ grep -rn "types_export" src-tauri/src/
(nada)
```

**Confirmado: o módulo não existe nesta árvore.** Portanto **nada confere `src/types.ts` contra as structs Rust**, e uma divergência de tipo passaria por `cargo check` **e** por `npm run build` os dois limpos. É por isso que a §3 abaixo foi feita à mão.

### Gate 8 — marcadores `SPEC:`

`grep -rn "SPEC:" src/ src-tauri/src/ | grep -i "LIB-"` → 9 arquivos com marcador. Todos os IDs citados estão em **LIB-01..LIB-12**; **nenhum ID inventado**. Cobertos: `src/App.tsx:1`, `src/components/Library/BookRow.tsx:1`, `LibraryPanel.tsx:1`, `src/components/Sidebar/LibrarySection.tsx:1`, `Sidebar.tsx:1`, `src/lib/libraryApi.ts:1`, `src/store/libraryStore.ts:1`, `src/store/uiStore.ts:3`, `src-tauri/src/library_commands.rs:1`. Mais `src-tauri/src/db.rs:1-3` (LIB-07) e `src-tauri/src/lib.rs:1-4` (LIB-02..LIB-12).

**Sobre `document_commands.rs` — eu estava errado na revisão 1. Ver §7.**

### Gate 9 — componentes órfãos da AD-052

Existem e **ninguém os renderiza**:

- `src/components/Documents/DocumentsPanel.tsx`, `DocumentRow.tsx`, `DocumentStatusBadge.tsx`, `src/store/documentsStore.ts` → presentes.
- `grep -rn "DocumentsPanel|DocumentRow|DocumentStatusBadge|documentsStore|DocumentsSection" src/` → as **únicas** referências são **internas ao próprio trio** (`DocumentsPanel.tsx:6-7,80`, `DocumentRow.tsx:3,28`) mais dois **comentários** em `BookRow.tsx:12` e `LibrarySection.tsx:19`. `src/App.tsx` não importa mais `DocumentsPanel` (`git diff` confirma a troca por `LibraryPanel`), e `Sidebar.tsx:20` renderiza `LibrarySection`.
- `src/components/Sidebar/DocumentsSection.tsx` está **apagado** (`git status`: ` D`).

**Confirmado: intactos e órfãos de rota, como a AD-052 manda.**

---

## 3. Conferência campo a campo Rust ↔ TypeScript (o ponto de maior risco)

Sem `types_export.rs`, esta é a **única** conferência que existe. Feita abrindo os dois lados.

### `BookRecord`

`src-tauri/src/library_commands.rs:96-103` (`#[derive(Serialize)]`, sem `rename_all`) ↔ `src/types.ts:141-150`

| Campo Rust | Tipo Rust | Campo TS | Tipo TS | Bate? |
| --- | --- | --- | --- | --- |
| `id` | `String` | `id` | `string` | ✅ |
| `filename` | `String` | `filename` | `string` | ✅ |
| `format` | `String` | `format` | `string` | ✅ |
| `size_bytes` | `u64` | `size_bytes` | `number` | ✅ (snake_case dos dois lados, como manda o `AGENTS.md`) |
| `imported_at` | `String` | `imported_at` | `string` | ✅ |

**5 campos, 5 batendo, nenhum a mais nem a menos.** Nenhum campo opcional dos dois lados.
Ressalva de tipo, não de nome: `u64` → `number` perde precisão acima de 2^53. Para tamanho de arquivo é inatingível (9 PB); registro por completude, não como defeito.

### `ImportBooksResult`

`library_commands.rs:107-111` ↔ `src/types.ts:152-156`

| Campo Rust | Tipo Rust | Campo TS | Tipo TS | Bate? |
| --- | --- | --- | --- | --- |
| `imported` | `Vec<BookRecord>` | `imported` | `BookRecord[]` | ✅ |
| `rejected` | `Vec<RejectedImport>` | `rejected` | `RejectedImport[]` | ✅ |

`RejectedImport` é **reusado**, não duplicado: `document_commands.rs:98-101` (`path: String`, `reason: String`) ↔ `src/types.ts:121-124` (`path: string`, `reason: string`). ✅

### Nomes de comando e de parâmetro

| `libraryApi.ts` | `#[tauri::command]` | Registrado no `invoke_handler` | Parâmetros | Bate? |
| --- | --- | --- | --- | --- |
| `invoke("import_books", { paths })` (`:7`) | `import_books` (`library_commands.rs:274`) | `lib.rs:152` | Rust: `paths: Vec<String>` (`:277`) — JS `paths` | ✅ |
| `invoke("list_books")` (`:8`) | `list_books` (`:289`) | `lib.rs:153` | só `db: State<DbState>` (injetado) | ✅ sem args |
| `invoke("delete_book", { id })` (`:9`) | `delete_book` (`:295`) | `lib.rs:154` | Rust: `id: String` — JS `id` | ✅ |
| `invoke("library_path")` (`:12`) | `library_path` (`:305`) | `lib.rs:155` | só `app: AppHandle` (injetado) | ✅ sem args |

Os quatro nomes de comando batem literalmente entre o `invoke`, a `fn` e o `invoke_handler`. Nenhum parâmetro precisa de conversão camelCase→snake_case (todos são uma palavra só), então a exceção do `AGENTS.md` não morde aqui.

**Resultado da conferência: nenhuma divergência.** Isto continua sendo verificação **humana**, não um gate — a próxima mudança numa dessas structs não tem nada que a acuse.

---

## 4. Defeitos encontrados

### D-01 — **CORRIGIDO** (verificado por mim na revisão 2) — a recusa mostrava o caminho inteiro em vez do nome, no Windows (LIB-03)

**Estado atual, medido:** `src/components/Library/LibraryPanel.tsx:92` agora é

```ts
name: item.path.split(/[\\/]/).pop() ?? item.path,
```

`[\\/]` é a classe "barra invertida **ou** barra normal" — o mesmo molde de `src/components/Documents/DocumentsPanel.tsx:69`, que era o padrão certo desta base. O caminho do Windows volta a ser reduzido ao basename.

**O que essa correção prova e o que não prova.** Ela é `IMPLEMENTED, NOT EXERCISED`: eu li o regex e ele está certo, e o `npm run build` continua exit 0 depois da mudança (bundle passou de `index-BhmqRmEJ.js` para `index-C5uO1JD5.js`, §2 gate 3). Mas **nenhum teste executa essa linha** — não há suíte de frontend nesta árvore — e o app não foi aberto. "Corrigido" aqui significa **o regex está certo à leitura**, não **visto na tela**. Se a T9 for feita, é barato confirmar: recusar um `.docx` no Windows deve mostrar `texto.docx`, não `C:\...\texto.docx`.

**Registro do defeito original**, para o histórico não sumir:

**Onde estava:** `src/components/Library/LibraryPanel.tsx:92`

```ts
name: item.path.split(/[\/]/).pop() ?? item.path,
```

O caractere `\/` dentro de uma classe de caracteres em regex JS é apenas **uma barra normal escapada**. A classe `[\/]` é equivalente a `[/]` — divide **só por `/`**, nunca por `\`.

**Comparação que revela a origem:** o componente de onde isso foi copiado está certo — `src/components/Documents/DocumentsPanel.tsx:69` usa `split(/[\\/]/)`, com a barra invertida. Uma barra invertida se perdeu na cópia.

**Cenário concreto de falha (Windows, plataforma-alvo):**

1. o usuário clica em "Importar livros" e escolhe `C:\Users\rafae\Downloads\texto.docx`;
2. o seletor devolve o caminho com barras invertidas;
3. `import_all` recusa e devolve `path` com o **caminho absoluto de origem** (`library_commands.rs:150-152` grava `source.to_string_lossy()`, não o basename);
4. o `split` não encontra nenhuma `/`, `pop()` devolve a string inteira;
5. a tela mostra **"Não importado: C:\Users\rafae\Downloads\texto.docx — formato não suportado…"** em vez de **"Não importado: texto.docx — …"**.

**Severidade:** cosmético, mas era exatamente o texto que o LIB-03 exige ("recusar aquele arquivo **pelo nome**"), e degradava mais ainda quando a seleção mistura vários arquivos longos. A correção foi de um caractere. **Nenhum gate pegava isto** — `tsc` e `vite` aceitam as duas regexes, e não há teste de frontend. Continua sem gate: se a barra invertida se perder de novo, nada acusa.

### D-02 (observação, não bloqueia) — `list_books` é disparado no boot, antes de haver banco

`src/components/Sidebar/LibrarySection.tsx:15-17` chama `loadBooks()` no mount, e o `Sidebar` monta sempre — inclusive durante o onboarding, antes de a pasta-base existir. `list_books` (`library_commands.rs:289-292`) vai falhar em `require_conn`, e o store grava a string em `error` (`libraryStore.ts:39-41`), que o `LibraryPanel` renderiza em vermelho (`:86`) quando o usuário abrir a Biblioteca. Além disso `LibraryPanel` chama `loadBooks()` de novo no próprio mount (`:29-32`), então há duas chamadas por abertura. Não medi o comportamento real — **é hipótese de leitura, para a T9 olhar**, não um defeito provado.

---

## 5. O que continua **sem prova de execução** — a lista que a T9 precisa fechar

Dito com todas as letras:

1. **O app nunca foi aberto.** `npm run tauri dev` não rodou nesta validação nem na implementação. **Nenhum `invoke` foi disparado.**
2. **`library_dir()` (`library_commands.rs:121-128`) nunca executou** — em teste nenhum. Isso derruba, sozinho, a prova de **LIB-04**, **LIB-11.3**, **LIB-11.4** e boa parte de **LIB-11/LIB-12**. Os testes de `import_all`/`remove_book` recebem a pasta já pronta (`:143`, `:253`), justamente contornando essa função.
3. **Os quatro comandos Tauri** (`import_books`, `list_books`, `delete_book`, `library_path`) **não têm teste algum** — não há runner de integração Tauri neste projeto. A ligação `invoke` ↔ `#[tauri::command]` está provada só por leitura (§3).
4. **Nenhum componente React foi renderizado** — nem em teste (não existe suíte: `npm test` sai com **código 1**, §2) nem no app. Isso inclui a lista (LIB-09), a linha de recusa (LIB-03, onde está o **D-01**), o caminho absoluto (LIB-12) e o botão de pasta (LIB-11).
5. **Nenhum arquivo real** `.mobi`, `.azw`, `.azw3` ou `.epub` passou pela detecção de DRM. Tudo foi montado byte a byte pelos testes (`:323-344`). LIB-05 e LIB-06 estão provados **contra o formato documentado**, não contra um arquivo produzido por um Kindle.
6. **A migração 9 não foi ensaiada contra cópia de banco real.** `db::real_database` continua entre os 15 `#[ignore]`. O `AGENTS.md` exige esse ensaio para migração destrutiva; esta é aditiva (`CREATE TABLE`), o que reduz o risco, mas o ensaio não aconteceu.
7. **O modo portátil (LIB-11.4) nunca foi montado.** A cadeia `base_path == ./data` → `./data/library` é dedução de leitura de `config.rs:45-47` sob a AD-034.
8. **PDF protegido por senha** passa por design (`library_commands.rs:35-37`, teste marcado como inconclusivo em `:609-618`) — está em Out of Scope, registro só para a T9 não confundir com defeito.
9. **`npm test` está quebrado nesta árvore** (exit 1, zero arquivos de teste, `src/test/` inexistente). Não é culpa desta feature, mas o `AGENTS.md` afirma o contrário e precisa ser reconciliado.

---

## 6. Arbitragem: o marcador `SPEC:` de `document_commands.rs`

Duas descrições divergiram e eu sou quem arbitra. **Medi as duas.**

### 6.1 O arquivo estava sem marcador? **Sim. Eu errei na revisão 1.**

```
$ git show HEAD:src-tauri/src/document_commands.rs | head -3
use crate::db::{require_conn, DbState};
use crate::rag::parsing;
use crate::rag::pipeline::{self, DocumentStatus};

$ git show HEAD:src-tauri/src/document_commands.rs | grep -n "SPEC:"
(nenhum marcador no HEAD)
```

O arquivo **nunca teve** marcador `SPEC:`. Na revisão 1 escrevi que "o marcador do topo não ganhou `book-library`", o que dá a entender que existia um marcador incompleto — **não existia marcador nenhum**. O outro agente mediu certo e eu descrevi errado. Registrado como correção minha, não dele.

Estado atual, medido agora:

```
$ head -1 src-tauri/src/document_commands.rs
// SPEC: documents-rag (DOC-02, DOC-03, DOC-08, DOC-09), book-library (LIB-02)
```

### 6.2 Os quatro IDs `DOC-` correspondem ao que o arquivo implementa? **Sim, os quatro. Nenhum ID inventado.**

Conferi cada um contra a tabela de `.specs/features/documents-rag/spec.md:121-132` **e** contra o que existe no arquivo:

| ID | Requisito na spec | O que implementa isso no arquivo | Bate? |
| --- | --- | --- | --- |
| `DOC-02` | "Copiar para pasta + registrar 'na fila'" | `import_documents` (`:113-114`) → `fs::copy` via `unique_destination` (`:37`) + `INSERT INTO documents` com `status = Queued` (`:177`, `:187-194`) | ✅ |
| `DOC-03` | "Rejeitar arquivo inválido/grande demais" | `parsing::is_supported` (`:131`) e `MAX_FILE_BYTES` (`:17`, checado em `:147-153`), devolvendo `RejectedImport` (`:98-101`, `:123-127`) | ✅ |
| `DOC-08` | "Listar documentos com status" | `list_documents` (`:223-224`) sobre o `SELECT … status …` de `:80` | ✅ |
| `DOC-09` | "Remover documento (arquivo + embeddings)" | `delete_document` (`:238-239`) | ✅ |

`LIB-02` também está correto: `unique_destination` (`:31-37`) é a função reusada pela biblioteca, e é a única razão de o arquivo ter sido tocado por esta feature.

**Uma incompletude, não uma violação:** o arquivo também abriga `requeue_unfinished_documents` (`:280`) e `discard_interrupted_attachments` (`:313`), que mexem em `chat_attachments` (`:91-94`) — território de outra feature, cujo ID não aparece no marcador. Isso é marcador **incompleto**, não ID inventado; a regra que a §3 de `.claude/rules/spec-driven-changes.md` proíbe é inventar, e não foi o caso. Fica como nota para quem tocar esse arquivo depois — **não** é trabalho desta feature e **não** entra no veredito.

---

## 7. Validation

**Result: FAIL** — a feature **não está pronta para ser marcada `Verified`**. O carimbo é do estado de verificação, não da qualidade do código.

**Por que FAIL, e não PASS.** O que está provado é real e eu mesmo executei: `cargo test --lib` 195/0/15, `cargo check --lib` zero warnings, `npm run build` exit 0, i18n 158/158, tipos Rust↔TS batendo campo a campo, nenhuma dependência nova, e 8 dos 12 requisitos com teste executado que falharia se o requisito quebrasse. O D-01 foi corrigido e a correção está certa à leitura.

Mas **quatro requisitos — LIB-01, LIB-04, LIB-11 e LIB-12 — não têm uma única linha de execução por trás**, e não é uma lacuna de forma: os três últimos dependem de `library_dir()` (`library_commands.rs:121-128`), que exige um `AppHandle` e **nunca rodou em teste nenhum**. Somando: o app nunca foi aberto, nenhum `invoke` foi disparado, os quatro comandos Tauri não têm teste, nenhum componente React foi renderizado (a suíte de frontend não existe — `npm test` sai com código **1**), e a correção do D-01 mora exatamente na camada que ninguém executa.

Um PASS aqui repetiria a AD-027 — seis requisitos dados como implementados quando só o backend existia e a UI não fechava o ciclo. Este relatório recusa isso de propósito.

**O que muda o carimbo para PASS:** a **T9** (UAT, do usuário) executando a lista da §5 — no mínimo LIB-01, LIB-04, LIB-11 e LIB-12, mais a checagem barata do D-01 (recusar um `.docx` no Windows deve mostrar `texto.docx`, não o caminho inteiro). Nada além disso está pendente do lado do código.

---

## 8. Veredito final

**Backend genuinamente provado e o D-01 corrigido — 195/0/15, zero warnings, build exit 0, i18n 158/158, tipos Rust↔TS batendo campo a campo, nenhuma dependência nova, tudo medido por mim — mas um terço dos requisitos (LIB-01, 04, 11, 12) segue sem uma única linha de execução, o app nunca foi aberto e `npm test` sai com código 1 nesta árvore: `FAIL` até a T9 rodar, e nenhum LIB-xx pode ir para `Verified` antes disso.**
