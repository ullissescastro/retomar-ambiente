# Retomar Ambiente

Aplicativo de código aberto para o COSMIC que pergunta, ao iniciar a sessão, se os aplicativos que **realmente estavam abertos** devem ser retomados.

## Requisitos

- Pop!_OS com o desktop COSMIC em uma sessão Wayland;
- Rust e Cargo para compilar;
- `just` para executar os comandos auxiliares;
- bibliotecas de desenvolvimento exigidas pelo `libcosmic`.

## Funcionalidades da versão 0.2.1

- registra apenas janelas de aplicativos marcados como elegíveis;
- no login, oferece somente os aplicativos que estavam abertos ao fim da sessão anterior;
- enumera também janelas que já estavam abertas quando o agente iniciou;
- registra o último monitor válido de cada janela, inclusive quando ela fica oculta em outro workspace;
- identifica monitores por fabricante/modelo e usa o conector como fallback;
- tenta devolver cada janela ao monitor em que estava;
- restaura, em melhor esforço, os estados maximizado, minimizado e tela cheia;
- diálogo **Retomar ambiente de trabalho?** com as ações **Restaurar aplicativos**, **Iniciar limpo** e **Gerenciar aplicativos**;
- perfil inicial com Firefox, Google Chrome, COSMIC Terminal, COSMIC Files e Visual Studio Code;
- descoberta dos lançadores `.desktop` instalados no sistema;
- pesquisa e seleção visual dos aplicativos elegíveis;
- ativação ou desativação da pergunta no login;
- configuração persistente pelo `cosmic-config`;
- instalação somente para o usuário, sem `sudo`.

O agente usa os protocolos Wayland oferecidos pelo COSMIC para observar janelas e solicitar movimentação entre monitores. O compositor pode ignorar uma solicitação, e alguns aplicativos podem criar janelas tarde demais ou com identificadores diferentes. Por isso, a restauração de monitor é de melhor esforço.

O conteúdo interno de cada aplicativo continua sob responsabilidade do próprio programa. Firefox, Chrome e VS Code, por exemplo, precisam estar configurados para restaurar suas próprias abas, janelas ou projetos.

## Arquivos de estado

Os retratos são armazenados em:

```text
~/.local/state/retomar-ambiente/current-session.json
~/.local/state/retomar-ambiente/previous-session.json
```

A lista contém apenas título, identificador do aplicativo, monitor e estados básicos da janela. Não contém conteúdo de documentos, abas ou histórico de navegação.

## Compilar e testar

```bash
just build-release
./target/release/retomar-ambiente
```

Abrir diretamente o gerenciador:

```bash
./target/release/retomar-ambiente --manage
```

Executar o agente manualmente para diagnóstico:

```bash
./target/release/retomar-ambiente --agent
```

## Instalar para o usuário atual

Para obter o código-fonte:

```bash
git clone https://github.com/ullissescastro/retomar-ambiente.git
cd retomar-ambiente
./install-user.sh
```

```bash
./install-user.sh
```

O binário é instalado em `~/.local/bin`, o lançador em `~/.local/share/applications` e as duas entradas XDG Autostart em `~/.config/autostart`.

## Teste recomendado

1. Instale a versão 0.2.1.
2. Deixe abertos apenas dois aplicativos elegíveis.
3. Aguarde alguns segundos e confira `current-session.json`.
4. Faça logout sem fechar esses aplicativos manualmente.
5. No login seguinte, confirme que apenas os dois aparecem no diálogo.
6. Clique em **Restaurar aplicativos** e confira os monitores.

## Remover

```bash
./uninstall-user.sh
```

As preferências e os retratos de sessão são preservados.

## Licença

MPL-2.0.
