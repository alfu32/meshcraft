# Meshcraft privacy notice

Updated: 28 October 2026

Meshcraft is a desktop 3D modeling application. The application does not require
a Meshcraft account. Its core modeling tools work without an internet connection.

## Projects and local settings

Meshcraft reads and writes the project files you choose. Projects can contain
geometry, materials, scripts, file references, names, annotations and IFC
properties. Such content may include personal information if you put it there.
The application also stores preferences, recent file paths, recovery data and
editing history locally. Imported files and referenced resources are processed
on your computer. The application's RAM, CPU and GPU usage counters are displayed
locally.

Meshcraft does not include an application analytics or advertising service and
does not automatically upload your projects or usage counters to its developers.
You control where you save or share your files. If you use a synchronized folder,
its provider handles synchronization under its own terms.

## Optional agent connection

You can start an optional MCP server in the application settings. It listens on
the local loopback interface, `127.0.0.1`, at the configured port. Compatible
clients on your computer can use that endpoint to inspect and edit the model,
read information exposed by its tools and execute supported scripts. Stop the
server in settings when you do not want that access.

Meshcraft does not include an external AI account or service. If you connect an
agent client, that client may send model content, prompts and tool results to its
own provider. Review the client's configuration and privacy terms before
connecting it. The local MCP endpoint itself does not select an external provider
or upload data to one.

## Websites, distribution and support

When you choose a website or support link, your browser opens that external
service. Microsoft Store downloads, purchases and account records are handled by
Microsoft under its own terms. GitHub handles repository visits and support
issues under its own terms. These services may collect data independently of the
Meshcraft application.

Support issues posted to the [public issue tracker](https://github.com/alfu32/meshcraft/issues)
are public. Do not include private projects, credentials or personal information
in a public report. Share only the information needed to explain the problem.

## Control and contact

You can remove saved project files, local preferences and recovery data from
your computer. Copies you shared or stored with another service are subject to
that service's controls and retention practices. Uninstalling the app may not
delete projects saved outside the app's own data folders.

For questions about this notice, use the
[Meshcraft issue tracker](https://github.com/alfu32/meshcraft/issues) without posting
sensitive information. This notice describes Meshcraft's current behavior and
will be updated when that behavior changes.
