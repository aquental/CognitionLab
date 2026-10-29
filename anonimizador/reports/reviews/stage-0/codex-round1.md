**C0-1 — Severidade: alta**  
**Arquivo:linha:** `anonimizador/src/anonimizador/cli.py:118`, também `:92`.  
**Descrição:** Os erros padrão do argparse imprimem argumentos rejeitados; a validação de idiomas também imprime integralmente valores fornecidos pelo usuário.  
**Impacto:** Caminhos e identificadores aparecem no stderr sem `--show-names`.  
**Reprodução:** `verify one.pdf --no-ocr /private/person/two.pdf` termina com código 2 e imprime `unrecognized arguments: /private/person/two.pdf`. `verify --lang /private/person x.pdf` também imprime esse caminho. Ambos foram reproduzidos.  
**Validação sugerida:** Testar argumentos intercalados, comando inválido, opções desconhecidas e idioma inválido com sentinelas confidenciais. Exigir código 2 e mensagens que usem somente posições e categorias.

**C0-2 — Severidade: alta**  
**Arquivo:linha:** `anonimizador/src/anonimizador/channels.py:52`.  
**Descrição:** Código zero não garante que o Poppler leu o canal. `run()` descarta o stderr mesmo quando contém um erro de decodificação. A correção G4-5 permanece incompleta nesse caso.  
**Impacto:** Um canal ilegível pode resultar em `ok` e código 0.  
**Reprodução:** Um PDF estruturalmente válido cujo conteúdo declara `/Filter /FlateDecode`, mas contém `invalidcompressedbytes`, faz `pdftotext` retornar 0, texto vazio e `Syntax Error (...): Unknown compression method in flate stream`. `pdfinfo` retorna 0. Com `--no-ocr`, nenhum desses erros chega a `Reading.errors`; a descompressão bruta também ignora a falha.  
**Validação sugerida:** Acrescentar um teste com esse PDF e exigir `ERROR`, código 2 e diagnóstico sanitizado, sem reproduzir o stderr original.

**C0-3 — Severidade: alta**  
**Arquivo:linha:** `anonimizador/src/anonimizador/channels.py:516`.  
**Descrição:** `raw_blobs()` tenta somente zlib diretamente sobre os bytes, sem interpretar `/Filter` ou cadeias de filtros.  
**Impacto:** Identificadores em streams antigos com filtros válidos ficam invisíveis, contrariando a cobertura declarada dos streams comprimidos.  
**Reprodução:** Acrescentar a um PDF limpo um objeto não referenciado com `/Filter [/ASCII85Decode /FlateDecode]`, contendo `BT (Zu) Tj (leo) Tj ET` comprimido com zlib e codificado em ASCII85. A busca retorna `raw=0`; o fluxo do CLI, adaptado para leitura em memória, retornou `ok` e código 0. O mesmo conteúdo como página ativa é extraído corretamente pelo Poppler.  
**Validação sugerida:** Cobrir cadeias de filtros em objetos antigos. Filtros não suportados devem impedir que o canal seja declarado limpo.

**C0-4 — Severidade: alta**  
**Arquivo:linha:** `anonimizador/src/anonimizador/channels.py:208`.  
**Descrição:** A correção G0-46 termina a imagem inline no primeiro ` EI ` encontrado, inclusive quando essa sequência pertence aos pixels.  
**Impacto:** Os pixels restantes voltam a ser interpretados como sintaxe; um `%` pode esconder o texto posterior. G0-46 ainda permite perder o identificador.  
**Reprodução:** Usar `BI /W 16 /H 1 /CS /G /BPC 8 ID `, seguido dos 16 bytes de amostra `b" EI %abcdefghijk"`, depois ` EI BT /F1 12 Tf 72 700 Td (Zu) Tj (leo) Tj ET`. O Poppler extrai `Zuleo` sem diagnóstico; a busca bruta conta zero. Como stream antigo comprimido junto de uma página limpa, retorna `ok` e código 0.  
**Validação sugerida:** Testar delimitadores aparentes dentro das amostras e preservar uma leitura conservadora quando o fim da imagem for ambíguo.

**C0-5 — Severidade: média**  
**Arquivo:linha:** `anonimizador/src/anonimizador/channels.py:316`.  
**Descrição:** G0-47 rejeita qualquer literal profundo que contenha a substring `stream`, mesmo quando ela é texto comum dentro do próprio literal.  
**Impacto:** Um nome escapado em um literal válido deixa de ser decodificado.  
**Reprodução:** `(Zul\351o (upstream (2)))` tem menos de 1 KB e não atravessa nenhum stream. O Poppler extrai `Zuléo (upstream (2))`, mas a busca bruta por `Zuleo` retorna zero. Em um `/Author` antigo não referenciado, o CLI retorna `ok` e código 0.  
**Validação sugerida:** Acrescentar esse caso e distinguir limites estruturais de streams das palavras contidas em strings. A correção G0-47 não pode ser considerada completa.

**C0-6 — Severidade: média**  
**Arquivo:linha:** `anonimizador/src/anonimizador/channels.py:187`, também `:253`.  
**Descrição:** O leitor hexadecimal exclui NUL dos espaços permitidos e exige pelo menos dois caracteres dentro dos delimitadores.  
**Impacto:** Strings hexadecimais que o Poppler lê corretamente podem ocultar identificadores em objetos antigos.  
**Reprodução:** `<5A\x0075\x006C\x0065\x006F>` é extraído como `Zuleo`, mas tem contagem bruta zero. `[<5><75><6C><61>] TJ` é extraído como `Pula`, também com contagem zero: `<5>` representa o byte `0x50` após completar o último nibble.  
**Validação sugerida:** Testar espaços NUL e strings de um único dígito hexadecimal, inclusive em objetos não referenciados, exigindo que o identificador seja encontrado.

**C0-7 — Severidade: média**  
**Arquivo:linha:** `anonimizador/src/anonimizador/channels.py:274`.  
**Descrição:** PDFDocEncoding é tratado como Latin-1, embora os caracteres não sejam equivalentes.  
**Impacto:** Caracteres de nomes desaparecem ou mudam antes da normalização, produzindo falsos resultados limpos em metadados antigos.  
**Reprodução:** `/Author (Fulano \223cticio)` é extraído por `pdfinfo` como `Fulano ﬁcticio`. A normalização dessa extração corresponde ao termo sintético `Fulano Ficticio`, mas `raw_readings()` conta zero porque interpreta `0x93` como controle Latin-1. Um autor antigo não referenciado depende exclusivamente dessa busca bruta.  
**Validação sugerida:** Decodificar PDFDocEncoding corretamente e testar caracteres que diferem de Latin-1, incluindo ligaturas, em objetos antigos.

**C0-8 — Severidade: média**  
**Arquivo:linha:** `anonimizador/src/anonimizador/verify.py:145`.  
**Descrição:** Os aliases são aplicados somente aos nomes encontrados originalmente; aliases que dependem de outro alias nunca são compostos.  
**Impacto:** A busca relativa original de G4-6 funciona, mas certas combinações de links ainda escondem nomes de diretórios.  
**Reprodução:** `root/a` aponta para um diretório com `clean.pdf`; `root/b` aponta para outro diretório contendo apenas `link -> primeiro diretório`; `root/zuleo -> root/b`. Na ordem de varredura `a`, `b`, `zuleo`, o resultado guarda `a/clean.pdf` e `b/link/clean.pdf`, mas perde `zuleo/link/clean.pdf`. O termo `Zuleo` não é encontrado.  
**Validação sugerida:** Cobrir esse encadeamento e compor aliases transitivamente, mantendo uma proteção explícita contra ciclos.

**C0-9 — Severidade: média**  
**Arquivo:linha:** `anonimizador/src/anonimizador/channels.py:634`.  
**Descrição:** Com OCR desativado, [redacted] não recebem nenhuma validação de formato ou integridade. Os leitores de metadados retornam uma lista vazia para bytes inválidos.  
**Impacto:** Um arquivo ilegível como imagem é declarado limpo.  
**Reprodução:** Um arquivo `neutral.jpg` contendo somente `not an image` produz `[1] ok` e código 0 com `--no-ocr`. Esse resultado foi reproduzido no fluxo do CLI com os bytes fornecidos em memória.  
**Validação sugerida:** Testar PNG/JPEG inválidos e truncados com `--no-ocr`; exigir `ERROR` e código 2. Desativar OCR não deve transformar uma falha de leitura em ausência de metadados.

**C0-10 — Severidade: alta**  
**Arquivo:linha:** `anonimizador/src/anonimizador/verify.py:193`.  
**Descrição:** A criação e a limpeza de `TemporaryDirectory` ficam fora do `try` que sanitiza `OSError`. A iteração de `verify()` também fica fora do tratamento de erros do CLI.  
**Impacto:** Falhas no diretório temporário causam traceback, código 1 e exposição do caminho presente na exceção.  
**Reprodução:** Fazer a criação do temporário levantar `PermissionError(13, "Permission denied", "/private/person/temp")`. A exceção atravessa `main()` sem tratamento. O mesmo ocorre com falhas de criação por falta de espaço.  
**Validação sugerida:** Simular falhas na entrada e na saída do contexto temporário; exigir código 2, mensagem sanitizada e ausência de traceback ou caminho.

**C0-11 — Severidade: alta**  
**Arquivo:linha:** `anonimizador/src/anonimizador/terms.py:156`.  
**Descrição:** A proteção do arquivo de termos verifica o caminho fornecido com `abspath`, mas não resolve o destino de links simbólicos.  
**Impacto:** Um arquivo de termos versionável dentro do repositório é aceito através de um link externo, contornando a proteção de privacidade de R1.  
**Reprodução ou justificativa verificável:** Criar fora do repositório um link para um arquivo de termos não ignorado dentro dele. `git -C` consulta o diretório externo e `committable()` retorna falso; em seguida, `open()` segue o link e lê o arquivo interno.  
**Validação sugerida:** Testar um link externo para termos versionáveis e verificar também o destino efetivamente aberto, recusando-o antes da leitura.

**C0-12 — Severidade: baixa**  
**Arquivo:linha:** `anonimizador/src/anonimizador/verify.py:234`, também `:228`.  
**Descrição:** O máximo entre leituras elimina duplicações de codificação, mas também trata blocos independentes como se fossem alternativas dos mesmos bytes.  
**Impacto:** As contagens de `metadata` e `raw` ficam menores que o número de ocorrências.  
**Reprodução:** Dois segmentos JPEG COM contendo individualmente `Zuleo` resultam em `metadata=1`. Dois streams Flate independentes contendo `<author>Zuleo</author>` resultam em `raw=1`. Ambos foram reproduzidos; existem duas ocorrências físicas em cada caso.  
**Validação sugerida:** Distinguir blocos independentes de suas alternativas de decodificação. Exigir contagem 2 nesses casos, mantendo contagem 1 para várias decodificações de uma única ocorrência.
