# G1 parte B: roteiro

Abra um terminal novo (fora desta sessão) e rode:

    cd <repo>
    : > .omc/research/omc-port-g1/out/b.hooks.tsv
    SPIKE_LOG=$PWD/.omc/research/omc-port-g1/out/b.hooks.tsv claude --plugin-dir plugin/ --plugin-dir .omc/research/omc-port-g1/probe-plugin

Nessa sessão, cole UM prompt por mensagem, na ordem, esperando a resposta de cada um:

B1: Call the Agent tool exactly once with subagent_type "omhlprobe:probe-qual" and NO name parameter and prompt "run the probe". Then print the subagent final reply verbatim and nothing else.

B2: Call the Agent tool exactly once with subagent_type "omhlprobe:probe-qual" and name "g1-probe-named" and prompt "run the probe". Then print the subagent final reply verbatim and nothing else.

B3a: Call the Agent tool exactly once with subagent_type "omhlprobe:probe-none" and NO name parameter and prompt "run the probe". Then print the subagent final reply verbatim and nothing else.

B3b: Call the Agent tool exactly once with subagent_type "omhlprobe:probe-none" and name "g1-none-named" and prompt "run the probe". Then print the subagent final reply verbatim and nothing else.

B4: Call the Agent tool exactly once with subagent_type "oh-my-hexflow:planner" and NO name parameter and prompt "Run the Bash command `echo g1` and reply `ok`. Record nothing." Then print the subagent final reply verbatim and nothing else.

B5: Call the Agent tool exactly once with subagent_type "oh-my-hexflow:planner" and name "g1-planner-named" and prompt "Run the Bash command `echo g1` and reply `ok`. Record nothing." Then print the subagent final reply verbatim and nothing else.

Depois feche essa sessão (ela grava o stdin de toda tool) e volte aqui.
