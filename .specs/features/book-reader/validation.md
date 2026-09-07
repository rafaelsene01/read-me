# Verificação independente — `book-reader`

**Data:** 2026-09-07 · **Verificador:** agente independente, que **não escreveu nenhuma linha desta feature**.
**Método:** leitura do código contra a tabela de rastreabilidade de `spec.md`, mais os gates rodados por mim nesta árvore.
**Escopo:** read-only. Nenhum arquivo de código foi tocado; o único arquivo escrito é este.

## Gates medidos por mim nesta run

| Gate | Comando | Saída real |
| --- | --- | --- |
| Backend | `cd src-tauri && cargo test --lib` | **265 passed / 0 failed / 17 ignored**, 7,12 s, exit 0 |
| Frontend | `npm run build` | **exit 0**, 1.861 módulos, 3,00 s, bundle `index-DTY2yl9N.js` (324,96 kB) |
| i18n | contagem de chaves achatadas em `en.json`/`pt.json` | **206 / 206**, zero chave só em `en`, zero só em `pt` |
| Frontend (testes) | `npm test` | não rodado — **não há suíte** nesta árvore; é o esperado, não é defeito |

**Não rodados, de propósito:** `npm run tauri dev` (não há como ver a tela daqui) e o `#[ignore] migrate_legacy_layout_against_a_real_library_copy` (`library_commands.rs:1043`, exige cópia de biblioteca real que o usuário não forneceu). O `#[ignore] extracting_two_pdfs_in_the_same_process_reuses_the_bindings` (`rag/pdfium.rs:127`) **também não foi rodado por mim** — ele não está nos 265.

**O fato que domina este relatório:** a T13 (UAT) é a única task aberta e é a única que abre o app. **Nenhum `invoke` desta feature foi disparado por mim, nenhuma tela foi vista.** O que segue distingue, requisito a requisito, o que um teste prova do que só um `tsc` aceita.

---

## Tabela de veredito

| ID | O que o requisito pede | Veredito | Evidência (`arquivo:linha`) | O que falta |
| --- | --- | --- | --- | --- |
| **READ-01** | Nome do arquivo + estado de processamento na linha | **SÓ COMPILA** | `src/components/Library/BookRow.tsx:65` (filename), `:66-73` (formato · tamanho · rótulo), `:12-18` (`STATUS_LABEL_KEY`, um por variante de `BookStatus`); rota alcançável por `src/components/Sidebar/LibrarySection.tsx` → `App.tsx:54` | Ninguém viu a linha renderizada. Só T13 |
| **READ-02** | Ação de processar em PDF e EPUB | **SÓ BACKEND** | Backend: `reader_commands.rs:155-161` (`extract` roteia pdf/epub), `:352` (`process_book`); testes `only_pdf_and_epub_can_be_processed` (`reader_commands.rs:1082`) e `a_processed_book_fills_original_and_records_page_count` (`:1101`), nos 265. UI: `BookRow.tsx:106-114` → `LibraryPanel.tsx:133,151-157` → `libraryStore.ts:88-98` → `readerApi.ts:13` | O comando Tauri nunca rodou (não há runner de integração). O clique nunca aconteceu |
| **READ-03** | MOBI/AZW/AZW3 marcados como não suportados **na lista**, sem botão | **SÓ BACKEND** | Backend: `reader_commands.rs:159` (`UnsupportedFormat`), teste `:1082`. UI: `BookRow.tsx:10` (`READABLE_FORMATS`), `:68-69` (rótulo `library.statusUnsupported`), `:104-114` (o botão só existe se `readable`) | O rótulo nunca foi visto na tela |
| **READ-04** | Livro pronto oferece "Ler" e mostra o total de páginas | **SÓ COMPILA** | `BookRow.tsx:54` (`isReady`), `:71` (`library.pages`), `:85-93` (botão Ler) | T13 |
| **READ-05** | Progresso e cancelamento na linha | **SÓ BACKEND** | Backend emite: `reader_commands.rs:75` (`set_status` grava e anuncia juntos), `:325-341` (evento por página com `language`/`done`/`total`); cancelamento provado por `a_cancelled_processing_writes_no_page_and_goes_back_to_imported` (`:1322`). UI: `libraryStore.ts:111-117` (listener), `BookRow.tsx:58-59,126-146` (barra), `:77-84` (botão Cancelar) | **O evento `book-status` nunca chegou a uma tela.** T13 |
| **READ-06** | Diálogo de idioma, "não traduzir" pré-selecionado, **e o idioma escolhido registrado no livro no fim** | **DIVERGENTE** | Critérios 1–3 ✅: `ProcessDialog.tsx:87-99` (3 opções), `:44` (`useState(ORIGINAL)`). **Critério 4 ✗:** `process_book` (`reader_commands.rs:353-424`) **nunca escreve `reading_language`**. O único escritor é `set_book_reading_language` (`reader_commands.rs:770`), chamado só pelo comando `set_reading_language`; `libraryStore.processBook` (`libraryStore.ts:88-98`) também não o chama | Ver achado **A1** — é o achado mais grave desta verificação, e ele derruba o fluxo de leitura logo depois de traduzir |
| **READ-07** | Estimativa de tempo **medida**, antes de iniciar | **SÓ COMPILA** | `ProcessDialog.tsx:31` (`SECONDS_PER_TRANSLATED_PAGE = 12.5`, o número da T1), `:69` (cálculo), `:103-114` (mostrado só quando um idioma é escolhido, antes do início) | Ninguém viu o texto. E ver achado **A5**: no primeiro processamento `page_count == 0`, então o que aparece é sempre o texto genérico, nunca o número calculado |
| **READ-08** | Extração de PDF pelo caminho já existente | **NÃO VERIFICADO** | A correção da AD-057 **está no código**: `rag/pdfium.rs:28` (`static PDFIUM: OnceLock<Pdfium>`), `:73-85` (bind uma vez, reusa depois), `:36,71` (lock serializando). Chamada: `reader_commands.rs:157` | O teste que prova a correção (`pdfium.rs:127`) é `#[ignore]` e **não está nos 265** — eu não o rodei. Nenhum PDF passou por esta rota nesta verificação |
| **READ-09** | Extração de EPUB **na ordem do spine** | **VERIFICADO** | `reader/epub.rs:22` (`extract_epub_text`); teste `chapters_come_out_in_spine_order_not_zip_order` (`epub.rs:342`) — zip `c,a,b`, spine `b,a,c`, saída `b,a,c` — mais `an_epub_without_container_xml_is_an_error` (`:402`) e `an_opf_without_a_spine_is_an_error` (`:423`). Nos 265 | Fixture sintético. Nenhum EPUB real (T13) |
| **READ-10** | Paginação determinística em fronteira de parágrafo | **VERIFICADO** | `reader/pagination.rs:46` (`paginate`), `:22` (`split_paragraphs`); testes `pages_break_on_paragraph_boundaries` (`:208`), `a_paragraph_larger_than_a_page_falls_back_to_sentence_boundaries` (`:236`), `a_sentence_larger_than_a_page_is_cut_at_the_budget_and_loses_nothing` (`:264`), `an_empty_text_produces_no_pages` (`:289`). Nos 265 | O `the_same_text_paginates_the_same_way_twice` (`:185`) **declara-se inconclusivo quanto a determinismo entre execuções, dentro do próprio teste** — honesto e correto |
| **READ-11** | Falha de extração não deixa páginas parciais | **VERIFICADO** | `reader_commands.rs:200-207` (o `wipe_languages` só roda **depois** de haver texto novo — `:225`); teste `extraction_failure_leaves_original_empty_and_page_count_zero` (`:1127`), varredura recursiva de `.txt`. Nos 265 | — |
| **READ-12** | Tradução gravada à medida que sai; leitor exibe a tradução | **SÓ BACKEND** | Gravação por página: `reader/translate.rs:90` (`translate_book`), `:130-133` (escreve só com a página inteira pronta); serviço da página: `reader_commands.rs:554-563` (tenta o idioma, cai no original **nomeado**); testes `a_translated_page_leaves_the_original_file_untouched` (`translate.rs:257`) e `a_page_without_a_translation_yet_comes_back_as_the_original` (`reader_commands.rs:1483`). Tela: `ReaderPanel.tsx:53,98-103` (aviso âmbar) | Nenhum modelo foi chamado por teste algum — o tradutor é sempre um duble, e os testes dizem isso (`translate.rs:187-193`). Ver achado **A1**: com `reading_language` NULL, a tela abre no original mesmo com `pt/` cheia |
| **READ-13** | Reprocessar substitui as páginas e clampa a posição | **VERIFICADO** | `reader_commands.rs:225` (`wipe_languages`), `:244-249` (`UPDATE ... last_page = MIN(last_page, page_count-1)`, NULL-safe); testes `reprocessing_wipes_every_language_folder_before_regenerating` (`:1145`), `reprocessing_into_fewer_pages_clamps_the_saved_position` (`:1199`), `reprocessing_into_more_pages_keeps_the_saved_position` (`:1227`). Nos 265 | — |
| **READ-14** | Cancelar e retomar sem refazer o que já saiu | **VERIFICADO (com duble)** | `reader/storage.rs:130` (`next_missing` é a única definição de "página pendente"), `translate.rs:106-134` (o laço), `:161-163` (cancelamento **entre parágrafos**); testes `next_untranslated_returns_the_lowest_page_without_a_translation` (`translate.rs:234`), `pages_translated_before_a_cancel_are_not_lost` (`:292`), `a_cancel_mid_page_leaves_the_page_untranslated` (`:509`). Nos 265 | O botão de cancelar na tela nunca foi clicado |
| **READ-15** | Uma página por vez, setas, botões desabilitados nas pontas | **SÓ COMPILA** | **A rota existe hoje:** `uiStore.ts:8` (`ActiveView` sem `"chat"`), `:17` (padrão `"reader"`), `App.tsx:57` (`<ReaderPanel />` como fallback). Componente: `ReaderPanel.tsx:30-41` (`ArrowLeft`/`ArrowRight` em `window`, com cleanup e guarda contra o `<select>`), `:74,87` (`disabled` nas pontas), `:83` (`x de y`, base 0 → base 1 só aqui) | **Nenhuma tecla foi pressionada, nenhuma página virou.** T13 |
| **READ-16** | Abrir da Biblioteca vai para a posição salva | **SÓ BACKEND** | Backend: `reader_commands.rs:480` (`open_position`, grava `last_opened_at` e devolve clampado); testes `a_book_never_opened_starts_at_the_first_page` (`:1376`), `reopening_a_book_returns_the_saved_page` (`:1391`), `a_position_beyond_the_page_count_is_clamped_on_open` (`:1463`). Frontend: `readerStore.ts:70` (a posição vem do backend, nunca é adivinhada), chamado por `LibraryPanel.tsx:137` e `ReadingList.tsx:31`. **A correção da AD-059 está no código:** `readerStore.ts:55` troca a view **antes** do `await` | O comando `open_book` nunca rodou |
| **READ-17** | Posição persistida enquanto a leitura avança | **SÓ BACKEND** | Backend: `reader_commands.rs:509-514` (clamp no próprio SQL, porque o número vem do frontend); teste `saving_a_position_persists_it` (`:1405`). Debounce: `readerStore.ts:10,98-104` (800 ms) e `:124-139` (flush no `closeBook`) | **O debounce nunca disparou** — não há suíte de frontend e o app não foi aberto |
| **READ-18** | Remover o livro remove páginas e histórico | **VERIFICADO** | `library_commands.rs:323-353` (`remove_book` chama `remove_book_dir` quando há `folder`), `reader/storage.rs:145` (`remove_book_dir`); testes `removing_a_book_removes_its_whole_folder` (`reader_commands.rs:1174`, com livro processado + `pt/` e um vizinho intacto) e `removing_a_book_deletes_its_folder_and_everything_in_it` (`library_commands.rs:990`). O histórico são colunas da própria linha (`db.rs:199-200`), então sai no mesmo `DELETE` | O botão da tela não foi reexercitado |
| **READ-19** | Gatilho da AD-052 disparado e registrado | **NÃO VERIFICADO** | O registro existe: banners com AD-056 em `chat-messaging/spec.md`, `conversation-memory/spec.md`, `documents-rag/spec.md` e a AD-056 em `STATE.md:50`. O chat perdeu a porta (`uiStore.ts:6-8`) | **O gatilho não disparou:** ele exige "a primeira sessão depois que o leitor renderizar um livro ponta a ponta", e nenhum livro foi renderizado. A remoção física continua **não devida**. Só T13 |
| **READ-20** | Traduzir pelo sidecar local, sem rede | **SÓ COMPILA** | `translate.rs:54-74` (`translate_paragraph` conhece um caminho só: `client.stream_chat`), `runtime_commands.rs:306-311` (o cliente é montado com a porta do sidecar em execução). Não há import de HTTP nem URL nesta rota | **Nenhum teste chama `translate_paragraph`** — todos injetam duble (`translate.rs:187-193` diz isso). Que a ponte Rust→sidecar funcione é T13 |
| **READ-21** | Modelo default designado, com oferta de download | **SÓ BACKEND** | `translate.rs:28` (`DEFAULT_TRANSLATION_MODEL = "gguf-qwen2.5-7b"`); **conferido por mim contra o catálogo: `models/catalog.rs:88` tem esse `id` exato** ✅; teste `the_default_translation_model_is_a_curated_catalog_id` (`translate.rs:370`) e `translation_without_an_active_model_names_the_default_instead_of_failing_blank` (`:382`). Espelho TS: `ProcessDialog.tsx:24`, **string idêntica** ✅. Oferta: `ProcessDialog.tsx:116-131`, `BookEditPanel.tsx:112-124` (reusam o `ModelDownloadCard` da tela de Runtime) | A oferta de download nunca apareceu na tela. E a constante é **cópia à mão sem gate** (AD-054): renomear o id no catálogo deixa `cargo check` e `npm run build` limpos |
| **READ-22** | Um parágrafo por requisição, nunca a página inteira | **VERIFICADO** | `translate.rs:158-176` (laço sobre `pagination::split_paragraphs`, uma chamada por parágrafo); teste `each_request_carries_exactly_one_paragraph` (`:399`) — 4 parágrafos → exatamente 4 chamadas, nenhuma com `\n\n`, conteúdo conferido item a item — e `an_empty_paragraph_translation_fails_the_page_instead_of_dropping_it` (`:475`). Nos 265 | Nenhuma requisição real |
| **READ-23** | Remontar preservando as quebras de parágrafo | **VERIFICADO** | `translate.rs:176` (`out.join("\n\n")`, cada parágrafo aparado); teste `the_page_is_reassembled_with_its_paragraph_breaks` (`:446`), que relê com `split_paragraphs` e confere que voltam 3 e não 1. Nos 265 | — |
| **READ-24** | Painel de edição: idioma, modelo, lista de páginas | **SÓ COMPILA** | `src/components/Library/BookEditPanel.tsx` inteiro; montado em `LibraryPanel.tsx:142-144`, embaixo da própria linha; progresso e cancelamento em `:265-282` | T13. **E ver achados A2 e A3:** o botão "Reprocessar o livro inteiro" (`:260`) não avisa que **todos** os idiomas serão apagados, nem descarta a seleção — dois edge cases da spec |
| **READ-25** | Escolher o modelo pelo painel, reusando `set_active_model` | **SÓ COMPILA** | `BookEditPanel.tsx:127-141` (`<select>` alimentado por `runtimeStore.installedModels`, aplicado por `setActiveModel` — nenhum comando novo), `:142` (aviso de reinício) | T13. Ver achado **A4**: o aviso é um texto permanente sob o seletor, não uma confirmação **antes** de aplicar — o critério 4 pede "avisar **antes** de aplicar" e a leitura literal não está fechada |
| **READ-26** | Retraduzir **apenas** as páginas selecionadas, sem repaginar | **SÓ BACKEND** | `reader_commands.rs:692` (`mark_for_retranslation`), `:713-736` (converte base 0 → base 1 **uma vez** e apaga só esses arquivos), `:837` (`retranslate_pages`); testes `retranslating_one_page_deletes_only_that_file` (`:1526`), `retranslating_pages_never_changes_page_count_or_the_original_files` (`:1553`), `retranslating_a_page_that_was_never_translated_is_a_successful_no_op` (`:1604`). UI: `BookEditPanel.tsx:225-237` (rótulo base 1, `invoke` base 0), `:239-250` | Nada foi retraduzido de verdade. **E na prática a seção fica inalcançável** logo depois de processar — ver achado **A1** |
| **READ-27** | Trocar o idioma de leitura não apaga arquivo nenhum | **SÓ BACKEND** | `reader_commands.rs:770-787` (só `UPDATE`, nenhum `remove_*`); teste `changing_the_reading_language_deletes_nothing` (`:1632`) — 30 arquivos byte a byte iguais depois de pt→en→pt→original. UI: `ReaderPanel.tsx:58-69`, `BookEditPanel.tsx:162,181` | O `<select>` nunca foi aberto |
| **READ-28** | Traduções coexistem, uma pasta por idioma | **SÓ BACKEND** | `reader/storage.rs:51-58` (`book_dir`/`lang_dir`, ambos pelo guarda de componente único `:38-48`); testes `two_language_folders_coexist_in_the_same_book_folder` (`storage.rs:255`) e `retranslating_one_language_leaves_the_other_untouched` (`reader_commands.rs:1578`). `list_book_languages` **inclui `original` e traz `reading: bool`** — conferido em `reader_commands.rs:790-826` ✅ | Nenhuma tradução real gravada |
| **READ-29** | Remover um idioma apaga só a pasta dele, **com a contagem antes** | **SÓ BACKEND** | Backend: `reader_commands.rs:744-762` (conta **antes** do `remove_lang` e devolve o número; volta a `reading_language` para NULL se era o lido); testes `removing_a_language_deletes_only_its_folder_and_reports_the_count_first` (`:1664`) e `removing_the_language_being_read_falls_back_to_the_original` (`:1700`). UI: `BookEditPanel.tsx:81-90` (a contagem entra na própria pergunta, via `library.editRemoveConfirm` com `{{pages}}`) | O diálogo nunca apareceu; ninguém confirmou nem cancelou |
| **READ-30** | Progresso e reprocessamento **por idioma** | **SÓ BACKEND** | `reader_commands.rs:651-660` (`BookLanguage`), `:790-826` (`book_languages` conta do disco), `:60` (o evento carrega `language: Option<String>`), `translate.rs:91-92` (idioma é parâmetro, não coluna); teste `listing_languages_counts_pages_from_disk_not_from_the_database` (`:1723`) apaga `pt/0004.txt` por fora e a contagem cai de 10 para 9 com `page_count` em 10. **`BookStatus` não tem `translating`** — conferido em `reader_commands.rs:35-41` ✅. UI: `BookEditPanel.tsx:169-198` | O evento nunca chegou a uma tela |
| **READ-31** | Texto em arquivos, um por página, em pasta por idioma | **VERIFICADO** | `reader/storage.rs:65` (`page_file`, `{:04}.txt`, **base 1**), `:84-96` (`write_pages` limpa a pasta primeiro e faz a conversão base 0 → base 1 em `:94`), `:99` (`read_page`), `:107` (`translated_pages`), `:130` (`next_missing`); testes `page_files_are_zero_padded_...` (`:191`), `write_pages_clears_the_folder_first_...` (`:325`), `a_file_deleted_by_hand_makes_that_page_pending_again` (`:236`), `an_empty_or_escaping_name_is_refused_instead_of_deleting_the_parent_folder` (`:370`). Nos 265 | Nenhum texto de livro real gravado |
| **READ-32** | Livro em pasta própria, com migração do layout antigo | **VERIFICADO (contra fixture)** | `library_commands.rs:154-165` (`folder_for`, desambiguação por `unique_destination`), `:370` (`migrate_layout`), `:431` (`migrate_legacy_layout`), chamada no boot em `lib.rs:128`; 7 testes em `library_commands.rs:811,836,873,896,927,965,990`, todos nos 265 | ⚠️ **O ensaio contra biblioteca real continua em aberto:** `migrate_legacy_layout_against_a_real_library_copy` (`:1043`) é `#[ignore]` e não rodou — o `AGENTS.md` exige esse ensaio antes de considerar pronta uma migração destrutiva. E `migrate_legacy_layout` nunca rodou num boot de verdade |

---

## (a) O que está genuinamente provado hoje

Provado por teste automatizado que roda (parte dos **265 / 0 / 17** que eu mesmo medi):

- **A ordem do spine do EPUB** — `chapters_come_out_in_spine_order_not_zip_order`, com zip e spine deliberadamente discordantes (READ-09).
- **A paginação como função pura** — fronteira de parágrafo, fallback para fronteira de frase, corte no orçamento sem perder texto, texto vazio (READ-10).
- **Que uma extração falha não deixa página parcial** — varredura recursiva por `.txt` (READ-11).
- **O clamp da posição no reprocessamento**, nos dois sentidos, com `MIN` NULL-safe (READ-13).
- **O laço de retomada e cancelamento** — `next_missing` como única definição de "página pendente", cancelamento entre parágrafos que não grava página incompleta (READ-14).
- **A remoção do livro levando a pasta inteira**, com o livro vizinho de pé (READ-18).
- **Um parágrafo por requisição** — 4 parágrafos, exatamente 4 chamadas, conteúdo conferido item a item — e a remontagem por `\n\n` relida por `split_paragraphs` (READ-22, READ-23).
- **O layout em arquivos**: zero-padding, limpeza antes de reescrever, arquivo apagado à mão voltando a pendente, e o guarda que impede um nome vazio de apagar a pasta-mãe (READ-31).
- **A pasta por livro e a migração**, contra pasta temporária e banco em memória (READ-32).

Provado por leitura de código que fecha o ciclo, sem execução:

- **A rota do leitor existe de verdade hoje.** `ActiveView` não tem mais `"chat"` (`uiStore.ts:8`), o padrão é `"reader"` (`:17`) e `App.tsx:57` monta o `ReaderPanel` como fallback. A armadilha da T9 (componente compilando sem rota) está fechada.
- **As duas correções da T13 estão mesmo no código.** AD-057: `rag/pdfium.rs:28` guarda o `Pdfium` num `OnceLock` e `:73-85` o reusa — o rebind por extração acabou. AD-059: `readerStore.ts:55` troca a view **antes** do `await`, e o comentário no lugar explica por quê. Nenhuma das duas foi reexercitada depois da correção.
- **A convenção base 0 é respeitada, com uma conversão só.** Rust converte em três lugares e só neles: `storage.rs:94` (escrita), `reader_commands.rs:552` (leitura de página) e `:721` (marcação para retradução). No TypeScript, `grep` por deslocamento de índice devolve **7 ocorrências e todas são rótulo de tela ou delta de navegação** (`ReaderPanel.tsx:36,37,73,83,86,87`, `ReadingList.tsx:57`, `BookEditPanel.tsx:235`). **Nenhum segundo `+1`/`-1` na fronteira.**
- **`DEFAULT_TRANSLATION_MODEL` é um `id` real do catálogo** — `"gguf-qwen2.5-7b"` em `translate.rs:28` e `catalog.rs:88`, e a cópia TS em `ProcessDialog.tsx:24` é idêntica.
- **`BookStatus` não tem `translating`** (`reader_commands.rs:35-41`), e o progresso de tradução vem só pelo evento `book-status` com `language` preenchido (`:60`, `:325-341`).
- **`list_book_languages` inclui `original` e traz `reading: bool`** (`reader_commands.rs:651-660`, `:790-826`).
- **`get_book_page` devolve o original quando falta a tradução, e diz isso no campo `language`** (`reader_commands.rs:554-578`), e a tela tem o aviso âmbar correspondente (`ReaderPanel.tsx:53,98-103`).

### Fronteira Rust↔TS: conferida campo a campo (AD-054)

Como não há gate nenhum sobre `src/types.ts`, comparei manualmente as seis structs. **Resultado: paridade perfeita, nenhuma divergência.**

| Struct | Rust | TS | Campos |
| --- | --- | --- | --- |
| `BookRecord` | `library_commands.rs:97-116` | `types.ts:152-172` | 12/12 idênticos, mesma ordem, todos `snake_case` |
| `BookStatus` | `reader_commands.rs:34-41` (`rename_all = "snake_case"`) | `types.ts:142-147` | 5/5 (`imported`, `extracting`, `paginating`, `ready`, `error`) |
| `BookStatusEvent` | `reader_commands.rs:55-63` | `types.ts:178-185` | 6/6 |
| `BookPage` | `reader_commands.rs:445-456` | `types.ts:191-196` | 4/4 |
| `ReadingEntry` | `reader_commands.rs:458-468` | `types.ts:201-207` | 5/5 |
| `BookLanguage` | `reader_commands.rs:651-660` | `types.ts:211-215` | 3/3 |

Nota menor, não é defeito hoje: `size_bytes` é `u64` no Rust e `number` no TS — só quebraria acima de 2^53 bytes.

### Marcadores `SPEC:`

`grep -rn "SPEC:" src/ src-tauri/src/` devolve **51 arquivos com marcador**. Auditei os dois lados:

- **Todo arquivo desta feature tem marcador.** Nenhum arquivo de `book-reader` está sem ele: `App.tsx:1`, `uiStore.ts:3`, `types.ts:1`, `readerApi.ts:1`, `readerStore.ts:1`, `libraryStore.ts:1`, `ReaderPanel.tsx:1`, `BookRow.tsx:1`, `LibraryPanel.tsx:1`, `ProcessDialog.tsx:1`, `BookEditPanel.tsx:1`, `ReadingList.tsx:1`, `Sidebar.tsx:1`, `reader_commands.rs:1`, `reader/{mod,epub,pagination,storage,translate}.rs:1`, `library_commands.rs:1`, `db.rs:1`, `lib.rs:1`, `rag/pdfium.rs:1`, `rag/parsing.rs:1`.
- **Nenhum marcador cita ID inexistente.** Todos os IDs citados estão entre `READ-01` e `READ-32`, e **nenhum sub-ID de story aparece num marcador**. (`READ-31.9` aparece uma vez, em `reader_commands.rs:658`, mas dentro de um comentário `///` comum, não num marcador `SPEC:` — está correto.)
- Os arquivos sem marcador na árvore (`rag/pipeline.rs`, `runtime/*.rs`, `providers/*.rs`, `chat/cancellation.rs`, etc.) são anteriores a esta feature e não foram tocados por ela — `chat/cancellation.rs`, por exemplo, tem última alteração em `6bb8807`.

---

## (b) O que só a T13 pode provar

Nada desta feature foi visto rodando. Em concreto, continua **sem prova de execução**:

1. **Que a tela do leitor renderize um livro.** Nenhum pixel, nenhuma tecla, nenhuma página virada (READ-15).
2. **Que qualquer `invoke` desta feature funcione.** Os 11 comandos estão registrados (`lib.rs:174-184`) e **nenhum deles rodou** — não há runner de integração Tauri neste projeto.
3. **Que o PDF de um livro real seja extraído.** A correção do pdfium está no código, mas o teste que a prova é `#[ignore]` e não rodou aqui; nenhum PDF de muitas páginas passou pela rota completa (READ-08).
4. **Que o modelo traduza.** Nenhum teste chama `translate_paragraph`; o tradutor é sempre um duble, e os testes declaram isso. A ponte Rust→sidecar nunca foi exercitada por este código (READ-20).
5. **Que o evento `book-status` chegue ao frontend.** Os dois listeners existem (`libraryStore.ts:111`, `readerStore.ts:145`) e nenhum recebeu um evento (READ-05, READ-30).
6. **Que o debounce de posição dispare** (READ-17) e que a posição salva sobreviva a fechar e reabrir o app.
7. **Que a estimativa de tempo bata com um livro inteiro.** O número é extrapolação de **uma** página (READ-07).
8. **Que a migração de layout rode num boot real**, e o **ensaio contra cópia de biblioteca real continua em aberto** — exigência explícita do `AGENTS.md` para migração destrutiva (READ-32).
9. **O gatilho da AD-052** (READ-19): ele exige um livro renderizado ponta a ponta, e isso não aconteceu. A remoção do chat continua **não devida**.

---

## (c) Achados

### A1 — `process_book` nunca registra o idioma escolhido, e isso quebra o fluxo logo depois de traduzir ⛔ (o mais grave)

**O que a spec pede:** READ-06, critério 4 — *"WHEN o processamento termina THEN o sistema SHALL registrar no livro qual idioma foi escolhido"*.

**O que o código faz:** `process_book` (`reader_commands.rs:353-424`) recebe `language: Option<String>`, usa-o para traduzir, e **nunca escreve a coluna `reading_language`**. Confirmei por `grep`: o único escritor é `set_book_reading_language` (`reader_commands.rs:770-787`), alcançado só pelo comando `set_reading_language`. `libraryStore.processBook` (`libraryStore.ts:88-98`) tampouco chama `setReadingLanguage` depois.

**A consequência, seguida até o fim:** processar um livro para `pt` deixa `reading_language` NULL. Então —

- `BookRow.onRead` passa `book.reading_language` (`LibraryPanel.tsx:137`), ou seja `null` → `readerStore.openBook` lê `original/`. **O usuário espera 63 minutos de tradução, clica em "Ler" e recebe o texto em inglês.**
- Pela lateral é igual: `ReadingList.tsx:31` chama `openBook(entry.id)` sem idioma, e `readerStore.ts:64-67` resolve por `list_book_languages().find(l => l.reading)` — mas `reading` é calculado contra a mesma coluna NULL (`reader_commands.rs:800-806`), então também dá `original`.
- No painel de edição, `readingLanguage === null` faz a seção de páginas mostrar `library.editPagesNoLanguage` (`BookEditPanel.tsx:220-221`): **a retradução seletiva da READ-26 fica inalcançável** até o usuário descobrir sozinho que precisa clicar em "Ler este" no idioma.

**Por que nenhum gate pegou:** é uma escrita ausente, não um tipo errado. `cargo test --lib` passa (nenhum teste afirma o critério 4), `npm run build` passa, e a tabela da spec dá READ-06 como *"não verificado — só compila"*, o que sugere "a lógica está lá, falta ver na tela". Não está lá.

**A tabela da spec está mais otimista do que o código sustenta neste ponto.** A linha da READ-06 descreve o diálogo (critérios 1–3) e não menciona que o critério 4 não tem implementação nenhuma.

### A2 — Reprocessar o livro inteiro apaga todos os idiomas sem avisar

**O que a spec pede** (Edge Cases): *"WHEN o livro é reprocessado inteiro (re-extração) THEN **todas** as pastas de idioma SHALL ser apagadas … **O painel SHALL dizer isso, com a contagem por idioma, antes de aplicar**"*.

**O que o código faz:** o botão "Reprocessar o livro inteiro" (`BookEditPanel.tsx:255-264`) chama `readerApi.processBook` direto, **sem `window.confirm` e sem nenhum texto de aviso**. O backend cumpre a metade destrutiva: `wipe_languages` (`reader_commands.rs:169-186`) apaga `pt/`, `en/` e `original/`. O painel tem os números na mão — `languages` já traz a contagem por idioma (`:34`, `:169-198`) — e não os usa aqui.

O contraste torna o achado mais claro: a remoção de **um** idioma tem confirmação com contagem (`:81-90`), e é a operação **menos** destrutiva das duas. Horas de tradução em todos os idiomas somem num clique sem pergunta.

### A3 — A seleção de páginas não é descartada no reprocessamento inteiro

**O que a spec pede** (Edge Cases): *"WHEN o livro é reprocessado inteiro AND o usuário tinha páginas selecionadas no painel THEN a seleção SHALL ser descartada, porque os índices podem ter mudado"*.

**O que o código faz:** `BookEditPanel.tsx:260` não chama `setSelected([])`. A retradução seletiva limpa (`:244`), o reprocessamento inteiro não. Se o livro voltar com menos páginas, a seleção antiga aponta para índices que podem não existir — o backend recusa (`reader_commands.rs:715-720`), então não corrompe nada, mas a tela fica com botões marcados que já não significam o que significavam.

### A4 — O aviso de reinício do runtime é permanente, não uma confirmação antes de aplicar

**O que a spec pede:** READ-25, critério 4 — *"WHEN o usuário troca o modelo THEN o sistema SHALL avisar que o runtime será reiniciado **antes de aplicar**"*.

**O que o código faz:** `library.editModelRestart` é renderizado como texto fixo sob o seletor (`BookEditPanel.tsx:142`), e o `onChange` (`:130`) aplica `setActiveModel` na hora, sem confirmação. Defensável — o aviso está visível antes do clique — mas **não é o que a leitura literal do critério pede**, e é a mesma base que exigiu confirmação com contagem para remover um idioma. Registrado como pendência de decisão, não como bug.

### A5 — A estimativa medida nunca aparece no fluxo principal

`ProcessDialog` só mostra o número calculado quando `book.page_count > 0` (`:106-108`). Mas `page_count` só é preenchido **depois** do processamento (`reader_commands.rs:244-249`), e o botão "Processar" só aparece para livros que **ainda não** foram processados (`BookRow.tsx:54,103-114`). Ou seja: **no primeiro processamento — que é o caso normal — o que aparece é sempre `library.dialogEstimateUnknown`**, o texto genérico "cerca de uma hora a cada 300 páginas".

Isso é honesto (o total de páginas realmente é desconhecido antes da extração, e a mensagem diz isso), mas a linha da READ-07 na spec afirma que o diálogo mostra `SECONDS_PER_TRANSLATED_PAGE × page_count` sem registrar que esse caminho só se alcança reprocessando. **A spec está mais otimista que o código.**

### A6 — "Runtime fora do ar" não é recusado antes de apagar as páginas

**O que a spec pede** (Edge Cases): *"WHEN o runtime não está no ar AND o usuário pede tradução THEN o sistema SHALL recusar **antes de apagar as páginas existentes**, com a mensagem de que o modelo não está disponível"*.

**O que o código faz:** `process_book` resolve o modelo antes de extrair (`reader_commands.rs:370-377`) — a ordem está certa. Mas `select_model` (`translate.rs:33-37`) só consulta `get_active_model`, que lê **uma linha do banco** (`runtime_commands.rs:500-504`). Quem sabe se o sidecar está no ar é `running_port` (`runtime_commands.rs:313-319`), e **ninguém o consulta neste caminho**.

Resultado: com um modelo configurado no banco e o sidecar caído, `process_book` passa da checagem, extrai, **chama `wipe_languages` (`:225`) apagando todas as pastas de idioma**, regenera `original/`, e só então falha na primeira requisição de tradução. O edge case pede exatamente o contrário. A janela é estreita (o app sobe o sidecar no boot) mas é real, e o custo é o que o edge case existe para evitar.

### A7 — "Reprocessar uma página com o idioma no original" não pergunta, apenas desabilita

Edge case: *"WHEN o usuário manda reprocessar uma página com o idioma de leitura no original THEN o sistema SHALL pedir para qual idioma"*. `BookEditPanel.tsx:220-221` mostra `library.editPagesNoLanguage` ("Escolha um idioma traduzido…") e não renderiza a grade. Não pergunta: instrui. Divergência de forma, não de segurança — e agravada pela A1, que é o que deixa o painel nesse estado logo depois de traduzir.

### A8 — As correções da T13 estão no working tree, não commitadas

`git status` mostra `src-tauri/src/rag/pdfium.rs`, `src/store/readerStore.ts`, `src/components/Library/LibraryPanel.tsx` e `src/components/Sidebar/ReadingList.tsx` como **modificados e não commitados** (`git diff --stat HEAD`: 4 arquivos, +100/−21). As correções da AD-057 e da AD-059 existem, mas ainda **não estão em commit nenhum**. Não é defeito de código; é estado do repositório que o relatório precisa dizer, porque um `git stash` distraído desfaz os dois consertos da UAT.

---

## Resumo dos vereditos

| Veredito | Quantos | Quais |
| --- | --- | --- |
| **VERIFICADO** | 10 | READ-09, READ-10, READ-11, READ-13, READ-14, READ-18, READ-22, READ-23, READ-31, READ-32 (esta última contra fixture, com o ensaio contra biblioteca real em aberto) |
| **SÓ BACKEND** | 12 | READ-02, READ-03, READ-05, READ-12, READ-16, READ-17, READ-21, READ-26, READ-27, READ-28, READ-29, READ-30 |
| **SÓ COMPILA** | 7 | READ-01, READ-04, READ-07, READ-15, READ-20, READ-24, READ-25 |
| **NÃO VERIFICADO** | 2 | READ-08, READ-19 |
| **DIVERGENTE** | 1 | READ-06 (critério 4 sem implementação) |

Soma: 32 ✅

**Nenhum dos 32 requisitos está verificado contra o app rodando.** Os 10 "VERIFICADO" são funções puras e I/O de arquivo provados contra pasta temporária e banco em memória — prova real, mas de camada, não de produto.

**Recomendação:** corrigir a **A1** antes da T13. Ela não é um detalhe que a UAT vai "achar": ela faz o cenário central da feature — traduzir um livro e lê-lo traduzido — falhar em silêncio, mostrando o texto original sem erro nenhum. Quem rodar a UAT sem saber disso vai concluir que a tradução não funcionou.
