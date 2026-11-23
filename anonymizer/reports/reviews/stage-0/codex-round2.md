**C0-1 — Severidade: média — correção incompleta**

**Arquivo:linha:** `anonimizador/src/anonimizador/channels.py:967`; `cli.py:124`.
**Descrição:** `missing_languages()` considera todas as palavras de `tesseract --list-langs` idiomas instalados, incluindo o caminho apresentado no cabeçalho. Esse valor passa pela validação e é impresso integralmente no relatório.
**Impacto:** Um caminho fornecido em `--lang` pode aparecer na saída sem `--show-names`.
**Reprodução:** Fornecer como idioma o token entre aspas do cabeçalho `List of available languages in "/private/Zuleo/tessdata/" ...`. Reproduzido também com o cabeçalho real da instalação local.
**Validação sugerida:** Interpretar somente as linhas de idiomas; testar um cabeçalho com caminho sentinela e exigir saída 2 sem divulgar o valor.

**C0-3 — Severidade: alta — correção incompleta**

**Arquivo:linha:** `anonimizador/src/anonimizador/channels.py:767`.
**Descrição:** Falhas dos decodificadores conhecidos são silenciadas. `unread` aumenta para filtros desconhecidos, mas não quando um filtro reconhecido falha. `flate_decode()` também aceita resultados parciais sem conferir `eof`.
**Impacto:** Um stream ilegível pode ser declarado limpo. C0-2 não protege objetos antigos que o Poppler não visita.
**Reprodução:** Acrescentar a um PDF limpo um objeto não referenciado com `/Filter /FlateDecode` e corpo `invalidcompressedbytes`. O fluxo do CLI em memória retornou `[1] ok`, código 0 e nenhum canal ilegível.
**Validação sugerida:** Cobrir filtros reconhecidos com dados inválidos e truncados, inclusive em objetos antigos. Exigir `ERROR`, código 2 e diagnóstico sanitizado.

**C0-4 — Severidade: alta — correção incompleta; G0-46 permanece aberta**

**Arquivo:linha:** `anonimizador/src/anonimizador/channels.py:484`.
**Descrição:** Quando não consegue calcular o tamanho das amostras, `inline_image_end()` ainda escolhe o primeiro ` EI `. Um espaço de cores nomeado nos recursos da página já provoca esse fallback.
**Impacto:** Bytes das amostras voltam a ser interpretados como sintaxe e podem esconder o identificador posterior.
**Reprodução:** Usar `/ColorSpace << /Cs1 /DeviceGray >>` nos recursos e `BI /W 16 /H 1 /CS /Cs1 /BPC 8 ID `, seguido de `b" EI %abcdefghijk"`, depois ` EI BT ... (Zu) Tj (leo) Tj ET`. O Poppler extrai o nome sem diagnóstico; a contagem bruta é zero. Como stream antigo comprimido, produz `ok` e saída 0.
**Validação sugerida:** Cobrir espaços de cores nomeados. Se o término continuar ambíguo, preservar leituras alternativas ou declarar o canal ilegível.

**C0-5 — Severidade: média — correção incompleta; G0-47 permanece aberta**

**Arquivo:linha:** `anonimizador/src/anonimizador/channels.py:384`.
**Descrição:** `STREAM_BOUNDARY` ainda interpreta a palavra `endstream` dentro de um literal como limite estrutural.
**Impacto:** Um identificador escapado no trecho externo do literal deixa de ser decodificado.
**Reprodução:** `/Author (Zul\351o (endstream (2)))`, abaixo de 1 KB. `pdfinfo` lê o autor corretamente, sem diagnóstico. Como objeto antigo não referenciado, a busca retorna zero e o CLI retorna `ok`, código 0.
**Validação sugerida:** Acrescentar esse literal ao teste de C0-5. Distinguir tokens estruturais de conteúdo de strings.

**C0-9 — Severidade: média — correção incompleta**

**Arquivo:linha:** `anonimizador/src/anonimizador/channels.py:919`.
**Descrição:** A validação PNG verifica os nomes dos chunks, mas não valida o conteúdo obrigatório nem a integridade dos pixels.
**Impacto:** Arquivos impossíveis de ler como imagem continuam sendo declarados limpos com `--no-ocr`.
**Reprodução:** A assinatura PNG seguida de `IHDR`, `IDAT` e `IEND`, todos vazios e com CRCs corretos, passa em `png_is_whole()`. Também passa um `IHDR` válido com `IDAT=b"broken"`. Ambos retornaram `ok` e código 0.
**Validação sugerida:** Testar cabeçalho inválido, dados comprimidos inválidos e quantidade insuficiente de pixels. Exigir `ERROR` e saída 2.

**C0-10 — Severidade: alta — correção incompleta**

**Arquivo:linha:** `anonimizador/src/anonimizador/verify.py:207`.
**Descrição:** `ignore_cleanup_errors=True` silencia falhas reais de remoção antes que alcancem o novo `except OSError`. O teste substitui o contexto inteiro e, portanto, não verifica esse comportamento.
**Impacto:** Renderizações e [redacted] extraídas podem permanecer no diretório temporário enquanto a execução informa sucesso.
**Reprodução:** Exercitar o handler real de `TemporaryDirectory`, simulando uma falha de E/S no callback de remoção. `read()` retorna `Reading.errors == []`. Reproduzido sem criar arquivos.
**Validação sugerida:** Não silenciar a limpeza. Testar o contexto real com a operação de remoção simulada, exigindo erro sanitizado e saída 2.

**C0-13 — Severidade: alta**

**Arquivo:linha:** `anonimizador/src/anonimizador/channels.py:761`.
**Descrição:** Uma cadeia composta exclusivamente por filtros Flate não entra no processamento sequencial. O código aplica apenas uma descompressão.
**Impacto:** Identificadores em streams validamente comprimidos duas vezes ficam invisíveis.
**Reprodução:** `/Filter [/FlateDecode /FlateDecode]` com `zlib.compress(zlib.compress(content))`, onde `content` desenha `(Zu) Tj (leo) Tj`. O Poppler extrai o nome sem diagnóstico; `raw` retorna zero. Como objeto antigo, o CLI retorna `ok`, código 0.
**Validação sugerida:** Executar todos os filtros declarados, inclusive repetidos. Acrescentar esse caso ao teste de cadeias de C0-3.

**C0-14 — Severidade: alta**

**Arquivo:linha:** `anonimizador/src/anonimizador/channels.py:740`.
**Descrição:** O reconhecimento de filtros omite declarações válidas: referência indireta, chave com escape hexadecimal e entrada situada antes da janela fixa de 4096 bytes.
**Impacto:** O stream é tratado como se não tivesse filtros; a tentativa isolada de zlib falha silenciosamente e nenhum erro é registrado.
**Reprodução:** Um conteúdo codificado com ASCII85 sobre Flate foi perdido nas três formas: `/Filter 10 0 R`, `/F#69lter [...]` e `/Filter [...]` seguido de uma entrada de preenchimento de 4200 bytes. O Poppler lê as três; como objetos antigos, todas retornaram saída 0.
**Validação sugerida:** Cobrir essas declarações. Resolver a estrutura corretamente ou registrar que o stream não pôde ser lido; nunca assumir ausência de filtro.

**C0-15 — Severidade: alta**

**Arquivo:linha:** `anonimizador/src/anonimizador/channels.py:770`.
**Descrição:** Os decodificadores recebem somente os bytes; `/DecodeParms` é ignorado. Isso altera o significado de Flate com predição e de LZW com `/EarlyChange 0`.
**Impacto:** Streams válidos são decodificados incorretamente, perdendo identificadores sem registrar erro.
**Reprodução:** Confirmado com Flate usando `/Predictor 2`, `/Colors 1`, `/BitsPerComponent 8` e `/Columns` correspondente ao conteúdo; também com LZW `/EarlyChange 0` após a mudança de largura dos códigos. O Poppler encontra o nome sem diagnóstico; os objetos antigos retornam `raw=0`, `unread=0` e saída 0.
**Validação sugerida:** Aplicar os parâmetros de cada filtro ou recusar explicitamente as combinações não suportadas.

**C0-16 — Severidade: alta**

**Arquivo:linha:** `anonimizador/src/anonimizador/channels.py:756`.
**Descrição:** O corpo do stream termina na primeira ocorrência dos bytes `endstream`, inclusive quando pertencem aos dados comprimidos. `/Length` não é considerado.
**Impacto:** Um stream válido é truncado antes do identificador, e a descompressão parcial é aceita.
**Reprodução:** Construir um zlib válido cujo primeiro bloco não comprimido contém `% endstream\n`, seguido de um bloco comprimido com o nome desenhado. O Poppler encontra o nome sem diagnóstico usando o comprimento declarado; a busca bruta retorna zero. Como objeto antigo, o CLI retorna `ok`, código 0.
**Validação sugerida:** Usar limites estruturais confiáveis e conferir o término da descompressão. Acrescentar um stream com esse marcador dentro dos dados.

**C0-17 — Severidade: média**

**Arquivo:linha:** `anonimizador/src/anonimizador/channels.py:850`.
**Descrição:** Um chunk PNG de metadados comprimidos que não pode ser descomprimido é descartado como se não contivesse metadados.
**Impacto:** Um lugar ilegível não gera `ERROR`, inclusive quando os pixels da imagem são perfeitamente válidos.
**Reprodução:** PNG válido com pixels íntegros e `zTXt` contendo `b"Comment\x00\x00invalidcompressedbytes"`, com CRC correto. A leitura retorna zero metadados e zero erros; o CLI retorna `ok`, código 0. O Tesseract também retorna 0, portanto ativar OCR não garante detectar a falha.
**Validação sugerida:** Interpretar os campos de `zTXt` e `iTXt`, distinguir conteúdo não comprimido e registrar falhas de descompressão como metadados ilegíveis, com saída 2.
