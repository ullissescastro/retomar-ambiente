# Histórico de versões

## 0.2.1 — 2026-08-04

Primeira versão pública.

### Funcionalidades

- registro dos aplicativos que permanecem abertos no fim da sessão;
- diálogo para restaurar aplicativos ou iniciar uma sessão limpa;
- gerenciamento dos aplicativos elegíveis;
- identificação do último monitor válido de cada janela;
- restauração de monitor em melhor esforço;
- restauração dos estados maximizado, minimizado e tela cheia;
- configuração persistente pelo `cosmic-config`;
- agente iniciado pela sessão gráfica;
- instalação somente para o usuário atual, sem `sudo`.

### Limitações conhecidas

- o compositor pode ignorar solicitações de movimentação de janelas;
- alguns aplicativos podem apresentar identificadores diferentes entre sessões;
- abas, documentos e projetos dependem da restauração do próprio aplicativo.
