# mud-client

A friendly terminal app for playing the text-based online game **Return of the Shadow (RoTS)**. It connects to the game and shows your health, mana, and stamina as easy-to-read bars, a live map of where you are, who you're fighting, and your group's status — all inside one window, alongside the game's normal text.

You do not need to know anything about Rust (the programming language it's built with) or command-line tools to use it. This page will get you playing; everything else is organized below so you can find deeper answers only when you want them.

## What does it look like?

The layout automatically adjusts to fit your window size — a small window gets a simplified view, and a large window gets the full experience shown below.

### Full-size window

![Normal layout with world information, map, opponent and character gauges beside MUD output](img/normal%20rust-client.png)

### Extra-wide window

![Ultrawide layout with world, social and nearby-map dashboard, group gauges and expanded map](img/ultra-wide%20rust-client.png)

### Tablet-sized window

![Tablet layout with map above MUD output, compact gauges and command input](img/tablet%20rust-client.png)

### Phone-sized window

![Mobile layout with a compact map above MUD output and command input](img/mobile%20rust-client.png)

## Getting started

You don't need to install any programming tools by hand — a guided setup script does that for you.

1. Download this project: click the green **Code** button on this page, choose **Download ZIP**, then extract (unzip) the folder anywhere on your computer.
2. Open the extracted folder and run the setup script for your computer:

   | Your computer | Do this |
   |---|---|
   | Windows | Double-click `Install-Windows.cmd` |
   | macOS | Double-click `Install-macOS.command` |
   | Linux (or Windows using WSL) | Open a terminal in the folder and run `bash install.sh` |

3. Follow the on-screen prompts. The script explains what it's doing and asks before it installs anything.
4. When it finishes, open a new terminal window and type `mud-client` to start playing.

That's it — no account, password, or extra setup is required to try it. The game connects automatically, and you can log in to your character right in the app's command box.

Need more detail, ran into an error, or want to set things up by hand instead? See the full [Installation Guide](docs/installation.md), which covers every step, computer type, and common problem.

## Everyday use

- **Playing on a specific character?** You can save separate settings per character; see [Character Profiles](docs/character-profiles.md).
- **Want to change how things look or work?** Almost everything is adjustable — colors, panel layout, and more. See the [Configuration Reference](docs/configuration.md).
- **Want the game to react automatically** (like auto-attacking or warning you when your health is low)? See [Scripting](docs/scripting.md) for simple, no-programming-required options.
- **Curious what a key or `/command` does?** See the [Command Reference](docs/commands.md).

## Learn more

Full documentation lives in [`docs/`](docs/index.md) and is organized by topic so you only need to read what's relevant to you:

| Guide | What it's for |
|---|---|
| [Installation](docs/installation.md) | Step-by-step setup for Windows, macOS, and Linux, including fixing common errors. |
| [Getting Started](docs/getting-started.md) | Installing, running, connecting, and reloading settings. |
| [Configuration Reference](docs/configuration.md) | Every setting you can change, with examples. |
| [Command Reference](docs/commands.md) | Every `/` command and what it does. |
| [User Interface](docs/ui.md) | How the screen layout, panels, and colors work. |
| [Mapper](docs/mapping.md) | The built-in live map, including how it draws rooms, doors, and paths. |
| [Scripting](docs/scripting.md) | Making the client react automatically to what happens in the game. |
| [Lua API Reference](docs/lua-api.md) | Advanced scripting reference for programmers. |
| [Lua Scripting Examples](docs/lua-scripting-examples.md) | Ready-to-use advanced scripting examples. |
| [Troubleshooting](docs/troubleshooting.md) | Fixes for common setup and connection problems. |

If you're a developer who wants to build the project from source, run tests, or contribute changes, see [Development and Validation](docs/development.md).

## License

Licensed under the [MIT License](LICENSE), copyright 2026 Seth Lyon. Commercial use, modification, and redistribution are permitted under its terms. Dependencies and third-party materials retain their own licenses; this does not relicense them.
