import pkgutil
import sys

import common
import core
import plugins
from invoke import Collection, Program


def list_plugins():
    for importer, modname, _ in pkgutil.iter_modules(plugins.__path__):
        mod = importer.find_module(modname).load_module(modname)
        yield (modname, mod)


class L12nProgram(Program):
    """A custom Program that doesn't print useless core options"""

    def print_help(self) -> None:
        print(
            f"""Usage: {self.binary} [--core-opts] <subcommand> [--subcommand-opts] ...

Core options:

  -e, --echo                     Echo executed commands before running.
  -h [STRING], --help[=STRING]   Show core or per-task help and exit.
  -V, --version                  Show version and exit.

Plugins:
"""
        )
        active = common.active_plugins()
        plugin_entry = lambda name: f"[{'x' if name in active else ' '}] {name}"
        self.print_columns([(plugin_entry(p[0]), p[1].__doc__) for p in list_plugins()])
        self.list_tasks()


if __name__ == "__main__":
    namespace = Collection.from_module(core)
    namespace.configure({"run": {"env": common.conf(core.VALIDATORS)}})

    plugin_set = common.active_plugins()
    # import all the modules in the plugins folder as collections
    for modname, mod in list_plugins():
        if modname in plugin_set:
            namespace.add_collection(Collection.from_module(mod))
            if hasattr(mod, "VALIDATORS"):
                namespace.configure({"run": {"env": common.conf(mod.VALIDATORS)}})
            plugin_set.remove(modname)

    if len(plugin_set) > 0:
        sys.exit(f"Unknown plugins: {plugin_set}")

    program = L12nProgram(
        binary="l12n",
        namespace=namespace,
        version="0.1.0",
    )
    program.run()
