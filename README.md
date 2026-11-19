# Sandbox V2 — MVP B | Rust + Bevy + GPU Compute

**Estado:** B0 e B1 aprovados (incluindo paridade Vulkan real). B2 em implementação na branch `feat/b2-surface-nets-gpu-resident-20261119`; B3–B7 não iniciadas.

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

- [B1: SDF determinístico, WGSL e testes de paridade](docs/B1_IMPLEMENTATION.md)
- [B2: extração Surface Nets residente na GPU](docs/B2_IMPLEMENTATION.md)

## Documentação

- [Especificação técnica do MVP B](docs/MVP_B_RUST_BEVY_GPU_COMPUTE.md)
- [Contrato comum de requisitos e benchmarks](docs/CRITERIOS_COMUNS.md)
- [Implementação e validação B0](docs/B0_IMPLEMENTATION.md)

## Sequência inicial

1. Fixar toolchain/dependências e obter um executável de janela 3D reproduzível.
2. Implementar o campo SDF e pipeline GPU de classificação, extração de malha e desenho.
3. Implantar streaming com LOD e buffers de região persistentes, sem readback maciço por frame.
4. Integrar edição, colisão local, rochas com física, água e persistência.
5. Automatizar percurso frio/quente e edição em movimento; medir CPU, GPU, memória e frame apresentado.

**Regra de continuidade:** estado do mundo independente do renderizador, buffers e versões por região, troca atômica de geometria pronta e proibição de descarte visual prematuro. Trabalhos extensos e readbacks não podem bloquear a thread principal.

## Execução

A B0 compilou e apresentou os dois cubos GPU no Windows/Vulkan. A B2 usa a mesma integração do MeshAllocator com GPU compute para gerar uma amostra estática de terreno SDF; o stream de 130m é B3. Para executar o artefato baixado do Actions, extraia-o preservando `assets/shaders/compute_mesh.wesl` e abra o terminal na raiz extraída antes de executar `./sandbox-game.exe`.

```powershell
git fetch origin
git switch feat/b0-rust-bevy-gpu-bootstrap-20261118
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\run-b0.ps1
```

A janela B0 mostra cubos gerados por compute shader em GPU, câmera com teclas WASD/Q/E (18 m/s), e métricas preliminares do Bevy. **Não** inclui terreno SDF, horizonte 130 m, streaming nem métricas completas de apresentação. O `Cargo.lock` já está versionado; o workflow compila usando `--locked`.


### B2 — execução local do artefato de CI

Extraia todo o pacote, incluindo `assets/shaders/sdf_density.wgsl` e
`assets/shaders/surface_nets.wgsl`, abra PowerShell na raiz extraída e execute:

```powershell
.\sandbox-game.exe
```

Variantes: `$env:SANDBOX_B2_LOD="0"` (grade de 0,5 m), `$env:SANDBOX_B2_SCENE="cave"`
(teste da caverna) e `$env:SANDBOX_STAGE="B0"` (prova anterior dos cubos).
No modo B2, o arquivo completo de shader deve acompanhar o executável.
