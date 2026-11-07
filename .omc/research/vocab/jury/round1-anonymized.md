# Júri: posições da rodada 1, anonimizadas

Enquadramento: [frame.md](frame.md). Condensado pelo foreman, sem alterar os argumentos. A ordem dos números foi embaralhada e não indica papel nem autor.

## Posição 1

**POSITION:** when=W-B; names=N-A.
**Adotar:** só as grafias oficiais dos hosts, nos vocabulários de host. **Emprestar:** `before`/`after` e os assuntos de host-events.md §5-6; prefixo por organização do CloudEvents; `sujeito.predicado` do CDEvents; PROV-O, Dublin Core e SACM só em `equivalent:`; MADR e OKF como nomes de campo.

1. W-A leva o agente a ler errado ou a duplicar o gate por host. O mesmo "antes do commit" vira `pre-commit` (git), `PreToolUse`+`Bash`+`if` (Claude Code), `beforeShellExecution` (Cursor), `tool.execute.before` (opencode). Um agente no Codex que lê `PreToolUse` não sabe que é o seu "antes de `git commit`". Um núcleo `{fase: before, assunto: commit}` é texto que o agente entende sem mapear nada.
2. W-C é o que mais induz leitura errada: os predicados do CDEvents são passado ou estado, então `change.merged` num `when` pode ser lido como "depois". Não há commit nem push, e a versão vai no `type`.
3. W-B pode crescer sem controle, mas o risco é limitável: lista pequena e fechada na v1 (fase `before|after`; assuntos `commit`, `push`, `pr-open`, `pr-merge`, `test`, `session`), uma glosa de uma linha por assunto, `matcher` opaco e por host, núcleo aditivo (só a base nativa escreve nele; os vocabulários de host seguem válidos).
4. Em N-A o `equivalent:` fica fora do texto do `SessionStart`. N-B injeta jargão que custa contexto. N-C amarra a um OKF de 3 meses sem release. Vetor de abuso de N-A: `equivalent:` errado ou desatualizado, por isso o motor o trata como informativo e nunca o consulta.

**EVIDENCE_GRADE:** B (notas primárias; lacunas: `if` do Claude Code não executado, exit 2 do Cursor é inferência, nenhuma medição de como um agente lê cada forma).
**ASSUMPTIONS:** o agente age pela leitura do texto; a lista do núcleo fica fechada e pequena; o `SessionStart` omite `equivalent:` e URIs; os vocabulários de host continuam disponíveis; nenhum padrão curado cobre "antes de", então inventar o núcleo é inevitável e declarado.
**CONFIDENCE:** 70

## Posição 2

**POSITION:** when=W-A; names=N-A.
**Adotar:** nada como dependência de código; entram como dado as grafias oficiais dos hosts, o formato `{vocabulary, event, matcher?}` e o campo opcional `equivalent: <URI>` (uma string). **Emprestar:** CloudEvents (prefixo), CDEvents (`sujeito.predicado`), PROV-O (`specializationOf`, `wasRevisionOf`, `wasInvalidatedBy`), Dublin Core (`replaces`/`isReplacedBy`), MADR, SACM, OTel, in-toto/SARIF (depois da v1).

1. W-A é o que as fontes sustentam. O matcher é heterogêneo (string ou regex nos agentes; objeto `{types, branches, paths}` no GitHub; ausente no Git; `if` só no Claude Code), então o matcher opaco comporta os 4 exemplos reais de host-events.md §8 sem normalizar. O CDEvents não serve de núcleo: sem commit nem push, nenhum "antes de", versão embutida no `type`, 0.x.
2. O núcleo de W-B é invenção de termos. `Stop` e `AfterAgent` bloqueiam e são "depois"; "antes do commit" nos agentes é uma chamada Bash cujo texto casa `git commit`, e um núcleo esconderia esse filtro onde ele muda de host. A v1 só injeta texto, então a camada de tradução não tem executor. W-B pode entrar depois como vocabulário `core` sem quebrar W-A.
3. N-A tem dependência zero. N-B: o PROV não tem `supersedes`, premissa nem evidência, `externalId` não tem termo, não há crate. N-C: OKF com 3 meses, sem release, links sem tipo; substituição do MADR é texto livre.
4. N-A dá referência fácil sem lock-in: cada termo carrega `equivalent: <URI>`.

**EVIDENCE_GRADE:** B.
**ASSUMPTIONS:** a v1 só injeta texto; não há núcleo declarativo comum entre hosts; nenhum padrão entrega os eventos do ciclo; assumida: o agente entende o `when` bruto do host sem ambiguidade (não testado); assumida e frágil: duplicar um mesmo gate por host é aceitável na v1 (se a base precisar do mesmo gate em vários hosts, W-B passa a compensar); adicionar `core` depois é aditivo.
**CONFIDENCE:** 76

## Posição 3

**POSITION:** when=W-A; names=N-C.
**Adotar:** (when) nomes de evento do CDEvents como vocabulário de dados `cdevents@0.5` (`change.*`, `build.*`, `testcaserun.*`), sem envelope, SDK nem versão no `type`; (names) campos de nó do OKF v0.2 (`type`, `title`, `description`, `sources`, `generated`/`verified`, `status`, `stale_after`) e campos da decisão do MADR 4.0.0 (`decision-makers`, `consulted`, `informed`, drivers, opções, outcome, consequências, confirmação; `status` só como ciclo de vida da decisão, nunca "superseded by"). **Emprestar:** prefixo do CloudEvents, PROV-O em `equivalent:` só para revisão, Dublin Core `replaces`, SACM, in-toto/SARIF no futuro.

1. O elo mais fraco de N-A é o `equivalent: <URI>`: nada o testa, nenhum consumidor falha se estiver errado, e as equivalências são inexatas (`wasRevisionOf` implica conteúdo substancial; o SACM foi conferido na 2.4 beta; o PROV-CONSTRAINTS não foi lido). Vira decoração que deriva.
2. Existe adoção barata e já em uso: o OKF é o formato do `wiki/` do harness (com ingestão e lint); o MADR custa só nomes de campo (MIT ou CC0); os pesquisadores deram "adotar subconjunto". Agentes já conhecem MADR.
3. Contra N-B a posição líder está certa. Na decisão 1, W-A está certo no essencial; a divergência é registrar `cdevents@0.5` como um vocabulário a mais para VCS, CI e deploy.

**EVIDENCE_GRADE:** C (o ganho de interoperabilidade não tem evidência de consumidores externos ao próprio harness; OKF com 3 meses e sem release, SACM só na 2.4 beta).
**ASSUMPTIONS:** o dono quer a adoção como contrato de formato; `status` é derivado da vigência ou restrito ao ciclo de vida próprio da decisão (se o "superseded by" do MADR for gravado, duplica a vigência calculada); o OKF v0.2 se mantém compatível; existe um consumidor real da metadata de ponte; o CDEvents continua sem "antes de".
**CONFIDENCE:** 52

## Posição 4

**POSITION:** when=W-A; names=N-A.
**Adotar:** nenhum termo de padrão externo como dependência; as grafias oficiais dos hosts como dado; `equivalent: <URI>` opcional, que o motor guarda e nunca lê. **Emprestar:** CloudEvents (prefixo), CDEvents (`sujeito.predicado`), PROV-O, Dublin Core, MADR, SACM, `status` do OKF.

1. Pre-mortem de W-C e W-B, 12 meses depois. W-C: o CDEvents muda de versão por tipo e não tem "antes de commit/push", então os gates centrais acabam em vocabulário de host de qualquer forma. W-B: a tabela de equivalência vira código do motor (N hosts por M assuntos), o mapeamento perde informação sem aviso (`Stop` é "antes de encerrar") e quem herda mantém uma taxonomia que muda no ritmo de 6 produtos alheios.
2. W-A não prende o motor a ritmos externos e é reversível: `vocabulary` é campo aberto, então um núcleo comum entra depois como mais um vocabulário sem migração; o contrário exige reescrever os gates nativos. Na v1 o núcleo não tem consumidor. Risco que sobra: o mesmo gate escrito 3 vezes e divergindo; mitigação: a base nativa começa em `free` e só ganha variantes por host quando houver hook real.
3. N-B: o PROV não tem `supersedes`, premissa nem evidência; `wasRevisionOf` pede conteúdo substancial; não há crate. N-C: OKF 0.2, sem release, links sem tipo, e o motor já tem relações tipadas, o que cria dois sistemas de link.
4. N-A: `equivalent` é metadado inerte. Pre-mortem: o campo apodrece ou promete mais do que o motor cumpre. Mitigações: validar só a sintaxe da URI, documentar como "aproximação, não equivalência formal", e um teste de arquitetura que impeça o motor de importar RDF.

**EVIDENCE_GRADE:** B.
**ASSUMPTIONS:** a v1 continua só-texto; `vocabulary` e `matcher` ficam como campos abertos; `equivalent` é opcional, informativo e nunca lido; poucos hosts na v1; guardar URIs como string não gera obrigação de licença (não verificado juridicamente); o dono aceita que "apoiar em padrões" signifique referência declarada.
**CONFIDENCE:** 74

## Posição 5

**POSITION:** when=W-A; names=N-A.
**Adotar:** nenhum termo externo como dependência de runtime. **Emprestar:** prefixo do CloudEvents e `sujeito.predicado` do CDEvents; a ideia de fase before/after; PROV `specializationOf`/`wasRevisionOf`; `dcterms:replaces`/`isReplacedBy`; campos do MADR; SACM 2.3 formal; `status`/`stale_after` do OKF. Pontes para padrão ainda móvel levam a versão no URI.

1. Reversibilidade: W-A só prende o motor ao formato `{vocabulary, event, matcher?}`, que o dono já fixou. W-B e W-C entram depois como um vocabulário a mais. Depois de W-B, voltar atrás é porta de mão única: a base nativa já estará escrita no núcleo e todo projeto que sobrepôs um nó nativo herda a abstração.
2. W-B é abstração sem consumidor na v1: "antes do commit" só é condição declarativa no Claude Code; mapear 7 hosts sem caso real que o valide faz o custo de manutenção crescer antes do ganho de portabilidade (que só vem com um segundo host de verdade).
3. W-C é a pior porta de mão única: 0.x, sem commit, push nem "antes de"; o dono teria que inventar um namespace `cdeventsx.*`.
4. N-A é a única que não depende de nada que mude: as pontes são metadado, trocáveis sem migração. N-B não cobre 5 das 8 relações do Hexlog. N-C amarra o envelope a um OKF de 3 meses e cria duas fontes de verdade para a substituição (o `status` do MADR contra a relação `supersedes` do motor).
5. Armadilha a documentar: nomes de tipo e de relação e o id de `vocabulary` ficam gravados em registros encadeados por hash e append-only; trocar um nome depois é criar revisão nova, então os nomes próprios precisam ser bem escolhidos na v1.

**EVIDENCE_GRADE:** B.
**ASSUMPTIONS:** a v1 roda só em Claude Code; o `when` pode mudar por revisão sobreposta sem quebrar referências ao id semântico; os tipos aceitam adicionar `equivalent` depois como metadado (não verificado); o `matcher` opaco não acumula a ponto de custar uma migração grande; o dono aceita `before-commit` escrito uma vez por host.
**CONFIDENCE:** 72
