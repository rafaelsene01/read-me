# Concerns

Riscos observados no código real (com caminho/arquivo como evidência), priorizados por impacto. Documentado após o M3 — nada aqui é bloqueante hoje, mas vários itens ficam mais caros quanto mais tarde forem resolvidos.

## Alto impacto

### C-01: ~~Schema SQLite não tem versionamento — alterar coluna existente não migra~~ — **RESOLVIDO POR IMPLEMENTAÇÃO (2026-07-25, M3.1)**

O fix sugerido aqui foi executado quase literalmente: `PRAGMA user_version` + `const MIGRATIONS: &[(u32, &str)]` aplicadas em ordem, cada uma em transação, rodando só as acima da versão atual (`src-tauri/src/db.rs`). A lista está hoje na **migração 8** (`MIGRATION_8_CHAT_MEMORY`), o que significa que sete mudanças de schema já atravessaram bancos existentes desde então — inclusive as destrutivas da 7, que derrubou `connections` e `model_configs`.

**Encontrado na auditoria de 2026-07-27** (run 001 da skill `spec-loop`): este item continuava listado em "Alto impacto", sem tachado, descrevendo o `db.rs` como "só uma sequência de `CREATE TABLE IF NOT EXISTS`" — enquanto o `ROADMAP.md` do M3.1 já dizia, desde 2026-07-25, *"resolve C-01 do CONCERNS.md"*. Os dois documentos se contradiziam havia dois dias.

**O risco trocou de forma, não desapareceu.** Ele agora mora no **número** da migração: duas entradas com o mesmo `u32` não quebram a compilação e não disparam teste — a segunda simplesmente nunca roda, porque o `user_version` já passou dela. O banco de quem escreve a migração, criado do zero, fica correto; o do usuário, que migra, fica sem a coluna. Por isso o `AGENTS.md` manda conferir o número na lista `MIGRATIONS` em vez de confiar na prosa.

### C-02: ~~`list_connections` faz health checks sequenciais de 5s — trava a UI~~ — **RESOLVIDO POR REMOÇÃO (2026-07-27, M9)**

Não existe mais lista de conexões para checar. O `runtime_status` lê uma linha de banco e consulta o estado do processo filho em memória; nenhum health check HTTP acontece ao abrir a sidebar. `list_connections`, `ConnectionManager` e os três clients com timeout de 5s foram apagados na AD-042.

### C-03: ~~`src/types.ts` espelha as structs Rust manualmente, sem geração~~ — **RESOLVIDO POR IMPLEMENTAÇÃO (2026-07-28, feature `generated-types`)**

**Como era:** `src/types.ts` replicava à mão structs de `providers/mod.rs`, `runtime/store.rs`, `models/catalog.rs` e `runtime_commands.rs` — 29 declarações escritas por pessoa. O caso mais frágil era `DownloadableModel`: no Rust `{ #[serde(flatten)] info: CuratedModelInfo, fits_ram: bool }`, no TS uma interface **plana** — a correspondência existia só por causa do `flatten`, e nada a verificava. Renomear um campo no Rust compilava dos dois lados; a quebra só aparecia em runtime, como `undefined` na tela.

**Como está:** `ts-rs = "12"` com `#[derive(TS)]` em **30 declarações**, geradas por `src-tauri/src/types_export.rs`. O `src/types.ts` traz cabeçalho `GENERATED FILE — do not edit by hand` e o comando de regeneração. O gate é o teste `types_export::tests::types_ts_matches_rust_structs`, que compara bytes e falha quando o Rust anda sem o TS acompanhar.

**A evidência é a mutação, não a contagem** — e foi feita duas vezes, por agentes diferentes:

1. O implementador renomeou `estimated_ram_gb` → `estimated_ram_gigabytes` em `models/catalog.rs`.
2. O orquestrador, independentemente, estreitou `Message.role` de `"user"|"assistant"|"system"` para `"user"|"assistant"` — uma mudança **puramente de atributo TS**, que não altera uma linha de Rust executável.

Nos dois casos **`cargo check --lib` termina limpo em ~2,5 s e `npm run build` também** — ou seja, os dois compiladores ficam calados diante da divergência, que é exatamente o risco que este item descrevia. O único a falar foi o gate:

```
first differing line (24):
  committed:  export type Message = { …, role: "user" | "assistant" | "system", … };
  generated:  export type Message = { …, role: "user" | "assistant", … };
```

Verde de volta após reverter: `181 passed; 0 failed; 16 ignored`.

**O que este item NÃO resolveu, e é honesto separar:** o gate garante que o `types.ts` corresponde às structs Rust. Ele **não** garante que a UI trate todos os valores da união. O `ChatAttachment.status` ficou mais largo depois da geração (`DocumentStatus | "injected_whole"`) e os dois usos no frontend são `=== "error"` / `!== "error"`, que compilam contra qualquer união — está registrado como todo no `STATE.md`.

### C-04: ~~Zero cobertura de teste no frontend~~ — **RESOLVIDO POR IMPLEMENTAÇÃO (2026-07-28, feature `frontend-testing`)**

**Como era:** `package.json` sem Vitest, Jest, Testing Library nem script `test`. **19 componentes React e 6 stores Zustand** (contados em 2026-07-27; eram 12 e 4 quando este item foi escrito) sem teste nenhum, incluindo as quatro lógicas que este item nomeava: o filtro `fits_ram` + toggle "mostrar todos" (`ModelsList.tsx`), o cálculo de percentual de download (`ModelDownloadCard.tsx`), o listener que indexa progresso pela URL do `.gguf` (`runtimeStore.ts`) e o listener de `memory-backfill-progress` que descarta eventos de outra conversa (`chatStore.ts`).

**Como está:** `npm test` → **8 arquivos, 63 testes passando, 2,89 s** (remedido pelo orquestrador em 2026-07-28, independente do agente que implementou). Vitest + jsdom + RTL, com `invoke`/`listen` interceptados por `test.alias`. **As quatro lógicas nomeadas acima estão cobertas**, e cada uma foi provada por mutação: quebrar a lógica de propósito derruba testes (12 mutações, 12 reprovações — a tabela está em `.specs/features/frontend-testing/tasks.md`). Um teste que continua verde com o código desligado não prova nada, e é por isso que a evidência aqui é a mutação e não a contagem.

**O que a cobertura já pagou:** um defeito real de produção que ninguém tinha visto — `sendMessage` grava o erro no `catch` e o `finally` chama `loadChats()`, que abre com `set({ error: null })`. Uma falha de envio fica visível por um tick e some antes de o React pintar: o usuário vê silêncio. Está fixado como teste de caracterização, escrito sobre a **sequência** de valores de `error`, para que consertar o store faça o teste falhar em vez de continuar verde. **Não foi corrigido** — o conserto muda comportamento de produção da `chat-messaging` e merece decisão própria.

**O que continua aberto** (não é mais C-04, é escopo novo): os **17 outros componentes** seguem sem teste — a feature cobriu os 2 que este item nomeava, os 5 stores e o `theme`. E `npm test` **não** está ligado ao `.github/workflows/ci.yml`, então a suíte só roda quando alguém a chama; o `- [ ]` está no `TESTING.md`.

## Médio impacto

### C-05: ~~Providers nunca exercitados contra servidor real~~ — **RESOLVIDO POR REMOÇÃO (2026-07-27, M9)**

Os dois clients que nunca tinham falado com um servidor de verdade (`OllamaClient`, `LmStudioClient`) são justamente os que saíram. O que restou — `LlamaServerClient` — fala com o sidecar que o próprio app sobe, e esse caminho já foi exercitado ao vivo na AD-028 e na AD-041.

### C-06: ~~Polling de download do LM Studio não tem timeout nem cancelamento~~ — **RESOLVIDO POR REMOÇÃO (2026-07-27, M9)**

O loop de polling saiu junto com `providers/lmstudio.rs`. Todo download agora é um GET direto de um `.gguf`, com progresso por bytes e sem estado de job para consultar.

### C-07: ~~`require_conn` duplicado em 3 arquivos~~ — **RESOLVIDO**

**Evidência:** a duplicação foi resolvida no caminho: `require_conn` vive em `db.rs` e é importada. Dois dos três arquivos que a copiavam (`connection_commands.rs`, `model_commands.rs`) nem existem mais.
**Status:** resolvido. Mantido aqui como registro.

### C-08: ~~Token de auth do LM Studio não é enviado~~ — **RESOLVIDO POR REMOÇÃO (2026-07-27, M9)**

Não há mais servidor externo a autenticar. O sidecar é filho do app, escuta em `127.0.0.1` numa porta efêmera e não usa credencial.

## Baixo impacto

### C-09: Sem linter nem formatter — CI resolvido pelo M8 (2026-07-26)

**Evidência:** não existem `.eslintrc*`, `.prettierrc*`, `rustfmt.toml` nem `clippy.toml`. `.github/workflows/` **passou a existir** com o M8: `ci.yml` roda `npm run build`, `cargo test` e valida Conventional Commits em todo push e PR. **Nunca foi executado no GitHub**, porém — o repositório ainda não teve um push que o dispare.
**Risco (o que sobrou):** estilo mantido só por disciplina manual. O build quebrado agora é pego pelo CI; o estilo divergente não.
**Fix sugerido:** `cargo clippy -D warnings` e `cargo fmt --check` foram deixados **de fora do M8 de propósito** (AD-034): o código atual não passa, e introduzi-los junto com o CI viraria uma refatoração disfarçada. Entram depois de pagar as dívidas — o `cargo check` de hoje ainda emite 5 warnings de dead code, incluindo o C-11.

### C-13: ~~Chaves estrangeiras declaradas mas nunca aplicadas~~ — RESOLVIDO (2026-07-26)

**Evidência (era):** `db::open` não executava `PRAGMA foreign_keys = ON`, e o SQLite deixa isso desligado por conexão. O `ON DELETE CASCADE` de `model_configs.connection_id` e a referência de `messages.chat_id` eram decorativos.
**Risco (era):** apagar um chat durante uma geração inseria a resposta num chat inexistente, como linha órfã silenciosa; e a primeira funcionalidade de apagar conexão herdaria `model_configs` órfãos confiando numa declaração que não valia.
**Resolução:** pragma ligado no `open`, com três testes — que o pragma está ativo, que o CASCADE dispara, e que uma mensagem órfã é recusada. Ver AD-040.

### C-14: ~~`delete_chat` não cancela a geração em andamento~~ — **RESOLVIDO (2026-07-27)**

**Evidência (era):** `commands.rs::delete_chat` apagava mensagens, anexos e o chat, mas não tocava no `CancellationRegistry`.
**Resolução:** `app.state::<CancellationRegistry>().cancel(&id)` como **primeira** linha do comando, antes da transação — a mesma via do `cancel_generation`. Sinalizar antes de apagar também estreita a janela que `chat::memory::record_turn` cobre com a checagem de existência: quanto antes o laço para, menos provável é ele chegar ao ponto de gravar memória.

**O que isto não tem:** teste automatizado. `delete_chat` é um comando Tauri que só orquestra I/O, e a matriz do `TESTING.md` põe isso explicitamente na coluna "nenhum teste" — não há runner de integração Tauri, e o comando precisa de um `AppHandle`. **A prova é de UAT e ainda não foi feita**: apagar um chat no meio de uma geração e observar o sidecar parar.

> **O M6 encostou nisto sem resolver (2026-07-27, AD-044).** A gravação de memória roda no fim da geração, então uma conversa apagada no meio poderia receber vetores num namespace que o `delete_chat` já limpou — órfãos que nada mais apagaria. `chat::memory::record_turn` confere que o chat ainda existe antes do `upsert`, o mesmo padrão do `still_exists` do pipeline. Isso fecha a janela nova; **a concern original continua aberta**.

### C-10: ~~Semeadura de conexão casa por `provider`, não por URL~~ — **RESOLVIDO POR REMOÇÃO (2026-07-27, M9)**

Não há semeadura: a tabela `connections` foi derrubada pela migração 7 e o runtime é um só, descoberto no `resource_dir` do próprio app.

### C-11: ~~Variantes `Quant::Q5/Q8/F16` sem uso (warning permanente no build)~~ — **RESOLVIDO (2026-07-27)**

**Evidência (era):** warning `variants Q5, Q8 and F16 are never constructed` em `cargo check` — os 6 modelos curados usam todos `Quant::Q4`.
**Resolução:** `#[allow(dead_code)]` explícito no enum, com o motivo escrito ao lado: a tabela descreve o **esquema de quantização**, não o catálogo atual, e apagar as variantes deixaria `estimate_ram_gb` especializada em Q4 continuando a se chamar como se fosse geral.

**Três warnings vizinhos foram varridos na mesma passada, e dois eram código morto de verdade:**

- `HEALTH_CHECK_TIMEOUT` e `LlamaServerClient::health_check` — sobras da tela de Conexões, que saiu com o M9 (AD-042). O único chamador restante era um teste, isto é, o método existia para que o teste tivesse o que chamar. Removidos os dois; o teste continua, exercitando `model_limits`.
- `PullStatus::Verifying` — a fase de checksum do `pull` do Ollama. Um GGUF baixado por um GET não tem essa fase. Removido no Rust **e** em `src/types.ts`, que espelha o enum à mão (C-03).
- Um `let mut` desnecessário num teste de `db.rs`.

**Estado:** `cargo check --lib` e `cargo check --lib --tests` passam com **zero warnings**. Isso é o pré-requisito que faltava para o C-09 poder ligar `clippy -D warnings` sem virar refatoração disfarçada — embora o `clippy` em si ainda não tenha sido rodado.

### C-12: Verificação só em Windows

**Evidência:** toda execução desta sessão foi `win32`; nunca houve build ou execução em Linux.
**Risco:** `tauri.conf.json` tem `"targets": "all"` e o roadmap promete `.AppImage`/`.deb` no M8, mas nada disso foi exercitado. Caminhos de filesystem usam `PathBuf`/`join` corretamente (portável), o que reduz o risco — mas é uma promessa não verificada.
**Fix sugerido:** entra naturalmente no M8 com a matrix do GitHub Actions.
