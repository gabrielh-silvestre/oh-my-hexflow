# Candidatos à base nativa: mineração de sessões (hexlog, work, pet-helper)

Plano de origem: [omhl-v2-graph-engine.md](../plans/omhl-v2-graph-engine.md). Os candidatos servem para semear a base nativa dos grafos de decisões e de gates/checks.

## Escopo e limites

- **Janela:** 2026-09-18 até 2026-11-07 (desde a última marca d'água da mineração).
- **Modo:** `markers`, 83 turnos mantidos de 2745 possíveis em modo `full`. 12 achados brutos de 3 lentes (haiku), 7 candidatos após a consolidação, 1 descartado.
- **Projetos:** `hexlog`, `work`, `pet-helper`.
- **Confidencialidade:** o `work` entra só como padrão genérico. Este arquivo não traz citações nem nomes de lá.
- **Viés:** a mineração procura o que deveria virar configuração do harness, não o que é regra genérica de desenvolvimento. Os candidatos 1 a 5 servem aos dois fins; 6 e 7 são do harness.
- **Nada foi aplicado e a marca d'água da mineração não avançou.** A janela continua disponível para o `harness-refine`.

## Candidatos

Tipos do grafo de decisões: premissa, diretriz, decisão. Os gates têm `when`, `how` e `blocking`; na v1 todos são só texto.

| # | Candidato | Tipo | when | how | blocking | Frequência | Entra na base nativa? |
|---|---|---|---|---|---|---|---|
| 1 | Verificação mecânica antes de commitar | gate | antes do commit | rodar os testes do projeto | sim | citado pelo dono (não minerado) | sim, é o gate de referência |
| 2 | Merge só com o CI verde | gate | antes do merge | esperar todos os check runs do head ficarem verdes; se um falhar, corrigir, dar push e rodar o CI de novo | sim | 3 sessões (hexlog) | sim |
| 3 | Conferir a ancoragem antes de postar comentários inline de review | gate | antes de publicar comentários de review | comparar path, line e side de cada comentário com o diff do head SHA; postar o texto aprovado sem alterar | sim | 6 sessões (work), padrão genérico | sim |
| 4 | Revisão de pendências em entrevista: uma decisão por turno | diretriz | ao percorrer pendências ou achados | uma pergunta por item, com 2 a 4 opções, esperando a resposta | não se aplica | 8 sessões, 3 projetos | sim |
| 5 | Política de commit e push no fim da tarefa | diretriz com padrão conservador | fim de uma tarefa de implementação verificada | padrão nativo: perguntar antes de commitar; o projeto sobrepõe para commitar e dar push sem perguntar | não se aplica | 34 sessões (hexlog) | sim, como exemplo de sobreposição por id |
| 6 | Repasse persistente para uma sessão limpa | diretriz | quando o usuário pede um repasse | gravar um arquivo com objetivo, contexto, decisões fechadas, pendências, branch e próximo passo; não executar a mudança na sessão atual | não se aplica | 10 sessões (hexlog, work) | depois da v1 |
| 7 | Trabalhar direto na `main`, sem branch nem PR | decisão de projeto | até começar a entrega de produto | commit e push na `main` | não se aplica | 1 sessão (pet-helper) | não, é exemplo de decisão do projeto, não nativa |
| 8 | Estado das revisões de PR só no ledger do líder da skill | decisão do harness | ao acompanhar revisões | não gravar o tracking na memória persistente | não se aplica | 1 sessão (hexlog) | não, é do harness |

## Descartado pela mineração

- "Texto postado em revisão não leva a linha de veredito no topo": apareceu em uma sessão só e não foi dito como regra permanente. Volta se aparecer numa segunda sessão.

## Pendências

- A frequência dos candidatos 5 e 4 vem de poucos projetos pessoais; a política de commit é diferente no `work` (precisa de pedido explícito), então o padrão nativo conservador está certo.
- Uma rodada em modo `full` (2745 turnos) acharia mais candidatos; ela custa mais e não foi feita.
- Os candidatos 3 a 5 e 8 são também candidatos de configuração do harness e seguem para o `harness-refine`, em outra sessão.
