# Sandbox V2 — MVP B | Rust + Bevy + GPU Compute

**Estado:** especificação inicial; implementação, build e benchmarks ainda não iniciados.

## Objetivo

Validar do zero um sandbox 3D de terreno volumétrico SDF editável, cavernas, streaming contínuo e renderização orientada à GPU, com rochas físicas e água básica. Este repositório não reutiliza código ou arquitetura de projetos anteriores.

## Stack proposta

- Rust stable fixado por `rust-toolchain.toml` e Bevy 0.20.x fixado por `Cargo.lock`.
- wgpu integrado ao renderizador Bevy para compute shaders WGSL, buffers persistentes e desenho indireto.
- Avian 3D para física, após validar uma combinação exata de versões compatíveis.
- Windows/Vulkan como plataforma inicial; fallback para hardware sem mesh shaders.

## Aceite obrigatório

| Métrica | Requisito |
| --- | --- |
| Frame completo | 60 FPS; orçamento de até **16,7 ms por frame** |
| Deslocamento | **18 m/s** contínuos, sem regiões desaparecendo ou piscando |
| Horizonte visual | **130 m de raio**, 360°, com LOD, terreno, rochas e água |
| Edição | **até 100 ms**, do comando ao frame que apresenta a alteração |
| Física | Colisões, gravidade e atrito de rochas; empuxo/arrasto e interação com água |
| Persistência | Restaurar terreno editado e estados alterados dos objetos |

O benchmark será medido no renderizador real, não apenas em tarefas de computação GPU ou testes headless.

## Documentação

- [Especificação técnica do MVP B](docs/MVP_B_RUST_BEVY_GPU_COMPUTE.md)
- [Contrato comum de requisitos e benchmarks](docs/CRITERIOS_COMUNS.md)

## Sequência inicial

1. Fixar toolchain/dependências e obter um executável de janela 3D reproduzível.
2. Implementar o campo SDF e pipeline GPU de classificação, extração de malha e desenho.
3. Implantar streaming com LOD e buffers de região persistentes, sem readback maciço por frame.
4. Integrar edição, colisão local, rochas com física, água e persistência.
5. Automatizar percurso frio/quente e edição em movimento; medir CPU, GPU, memória e frame apresentado.

**Regra de continuidade:** estado do mundo independente do renderizador, buffers e versões por região, troca atômica de geometria pronta e proibição de descarte visual prematuro. Trabalhos extensos e readbacks não podem bloquear a thread principal.

## Execução

Ainda não há executável ou comandos de build validados. Eles serão documentados após a primeira compilação reproduzível.
