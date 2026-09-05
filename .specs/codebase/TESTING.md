# Testing

**Status (2026-07-28):** **63 testes de frontend passando em 8 arquivos** (`npm test` com Vitest, 3,08 s — medido em 2026-07-28), **181 testes Rust passando, 0 falhas e 16 `#[ignore]`** (remedido na run 002 com `cargo test --lib`, 7,03 s; a contagem aqui dizia 177/15, que era o baseline **antes** de a feature `generated-types` acrescentar os 4 testes do comparador de tipos + 1 `#[ignore]`), mais **49 testes de script** em `node --test` (`npm run test:scripts`, 115 ms — remedido em 2026-07-28; a contagem anterior aqui dizia 43).

> **Este cabeçalho dizia "Sem suíte de testes automatizada ainda" até 2026-07-27**, quando o M6 foi implementado — muito depois de a suíte existir e crescer para quase 170 testes. Era exatamente a divergência que o `AGENTS.md` manda corrigir: um leitor podia acreditar no documento e concluir que não havia nada para rodar. Corrigido junto com a AD-044.
>
> **E dizia "Frontend continua sem runner" até 2026-07-28**, quando a feature `frontend-testing` já tinha instalado Vitest + jsdom + React Testing Library e escrito 63 testes. Mesma divergência, um dia depois — a correção veio junto com a T9 daquela feature.

## Gate Check Commands

| Gate | Command | O que valida |
| --- | --- | --- |
| `quick` (Rust) | `cd src-tauri && cargo test --lib <módulo>::` | O módulo que a task mexeu |
| `quick` (scripts) | `npm run test:scripts` | Os scripts Node de release e vendoring |
| `quick` (frontend) | `npm test` (ou `npm run test:watch`) | Stores Zustand, listeners de evento e a lógica de apresentação coberta pela `frontend-testing` |
| `build` (Rust) | `cd src-tauri && cargo check` | Backend compila, sem erros de tipo/borrow checker |
| `build` (frontend) | `npm run build` | `tsc` sem erros + Vite builda |
| `full` | `cargo test --lib` inteiro + `npm run tauri dev` até log mostrar `Finished` + `Running` sem erro, processo de pé | Suíte completa e app real subindo ponta a ponta |

**Os `#[ignore]` não são testes desligados** — são os que tocam um recurso real e por isso não pesam na suíte padrão. Dois formatos em uso: o que **cria** o recurso numa pasta temporária (`rag::store`, `chat::memory`) e o que usa um recurso **já existente na máquina**, sempre por variável de ambiente e nunca por caminho adivinhado (`db::real_database`, `runtime::detect::detect_real`, `runtime::process::sidecar_real`). Rodar: `cargo test --lib <módulo> -- --ignored`.

## Test Coverage Matrix

| Code Layer | Test Type Required | Justificativa |
| --- | --- | --- |
| Funções puras Rust (parsing, fórmula de RAM, chunking, montagem de contexto) | unit (`cargo test`) | Lógica testável sem I/O; barato de cobrir, alto valor (bugs aqui corrompem RAG silenciosamente) |
| Comandos Tauri (`#[tauri::command]`) que só orquestram I/O (DB, HTTP, filesystem) | none (por ora) | Sem test runner de integração Tauri configurado ainda; verificação é manual via `tauri dev` |
| Stores Zustand e funções puras de frontend (listeners de evento, normalização de tema) | unit (`npm test`, Vitest) — obrigatório | É onde mora a lógica que o `tsc` não vê: escopo por `chatId`, chave do progresso de download, id do documento. Os `listen(...)` são registrados **no import do módulo**, então o único jeito de exercitá-los é interceptar `@tauri-apps/api` por `test.alias` (ver `design.md` da `frontend-testing`) |
| Componentes React | unit (Vitest + RTL) **só onde a apresentação decide o que o usuário vê**; nenhum para o resto | Cobrir toda a árvore de JSX é cerimônia; cobertos hoje: o filtro `fits_ram` do `ModelsList` e o percentual do `ModelDownloadCard`, os dois que o C-04 nomeia. Sem snapshot e sem threshold de cobertura — o gate é mutação |
| Parsers de documento / o cliente HTTP do `llama-server` | unit com fixtures/mocks quando prático | Evita depender de um sidecar de pé para `cargo test` passar; o caminho real é coberto pelos `#[ignore]` |

**Parallelism Assessment:** `cargo test` é seguro para paralelizar (Rust testa em threads por padrão), e os `#[ignore]` que escrevem em disco derivam o diretório do PID do processo, então não colidem entre si. Tasks marcadas `[P]` só precisam não compartilhar o mesmo arquivo.

## Onde os testes moram

`#[cfg(test)] mod tests` no **fim do mesmo arquivo**, nunca num diretório `tests/` separado. Uma função que precisa de teste e não é testável sem `AppHandle` é sinal de que falta extrair a decisão como função pura — foi assim que nasceram `should_record_turn` e `recall_blocks` no M6, e `classify_output` antes deles.

No frontend vale o mesmo princípio, com a sintaxe da linguagem: o teste fica **ao lado** do arquivo que testa (`src/store/chatStore.test.ts`, `src/components/Runtime/ModelsList.test.tsx`), não numa árvore `__tests__/`. A infraestrutura compartilhada mora em `src/test/`: `setup.ts` (limpeza do RTL e reset dos stubs) e `doubles/` (os substitutos de `@tauri-apps/api/core` e `/event`, ligados por `alias` no `vitest.config.ts`).

O que a suíte de frontend **não** faz, de propósito: snapshot de JSX, threshold de cobertura, e qualquer teste que suba o app inteiro — renderizar `App.tsx` exigiria stubar todos os comandos de boot e viraria UAT disfarçado de unidade.

## Todo

- [x] ~~Introduzir `cargo test` para os módulos de lógica pura~~ — feito e mantido desde então; a linha de base atual é 177
- [x] ~~Avaliar Vitest/React Testing Library quando a superfície de componentes estabilizar (é o C-04 do CONCERNS.md, e o toggle de memória do M6 é mais uma superfície sem cobertura)~~ — feito pela feature `frontend-testing` em 2026-07-28: `vitest@4.1.10` + `jsdom@29.1.1` + `@testing-library/react@16.3.2`, 63 testes, cada lógica coberta validada por mutação. A linha fica riscada em vez de apagada porque o "por quê" da espera (superfície instável) explica por que a cobertura chegou tão depois do backend
- [ ] Ligar `npm test` no `.github/workflows/ci.yml` — hoje a suíte de frontend só roda quando alguém a chama. Ficou fora da `frontend-testing` porque aquela task não podia tocar `.github/`
