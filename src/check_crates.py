# /// script
# dependencies = [
#   "requests",
#   "rich",
# ]
# ///

import requests
import string
from concurrent.futures import ThreadPoolExecutor, as_completed
from rich.console import Console
from rich.progress import (
    Progress,
    SpinnerColumn,
    TextColumn,
    BarColumn,
    TaskProgressColumn,
)
from rich.table import Table

# crates.io Sparse Index URL for 1-2 letter crates: https://index.crates.io/2/{crate_name}
BASE_URL = "https://index.crates.io/2/{}"


def check_crate(name):
    url = BASE_URL.format(name)
    try:
        # We only need the headers to check for 200 (exists) or 404 (available)
        response = requests.head(url, timeout=5)
        if response.status_code == 404:
            return name, True  # Available
        elif response.status_code == 200:
            return name, False  # Taken
        else:
            return name, None  # Error
    except requests.RequestException:
        return name, None


def main():
    console = Console()
    letters = string.ascii_lowercase
    combinations = [a + b for a in letters for b in letters]

    available = []
    taken_count = 0
    error_count = 0

    console.print("[bold blue]Crates.io 2-Letter Availability Checker[/bold blue]\n")

    with Progress(
        SpinnerColumn(),
        TextColumn("[progress.description]{task.description}"),
        BarColumn(),
        TaskProgressColumn(),
        console=console,
    ) as progress:
        task = progress.add_task("[cyan]Checking crates...", total=len(combinations))

        with ThreadPoolExecutor(max_workers=20) as executor:
            future_to_crate = {
                executor.submit(check_crate, name): name for name in combinations
            }

            for future in as_completed(future_to_crate):
                name, is_available = future.result()

                if is_available is True:
                    available.append(name)
                    progress.console.print(
                        f"[green]✔[/green] Found available: [bold green]{name}[/bold green]"
                    )
                elif is_available is False:
                    taken_count += 1
                else:
                    error_count += 1
                    progress.console.print(f"[red]✘[/red] Error checking: {name}")

                progress.update(
                    task, advance=1, description=f"[cyan]Checking {name}..."
                )

    # Final Summary Table
    console.print("\n[bold]Summary Results[/bold]")
    table = Table(show_header=True, header_style="bold magenta")
    table.add_column("Status", style="dim")
    table.add_column("Count", justify="right")

    table.add_row("Available", f"[bold green]{len(available)}[/bold green]")
    table.add_row("Taken", str(taken_count))
    table.add_row("Errors", f"[red]{error_count}[/red]" if error_count > 0 else "0")

    console.print(table)

    if available:
        console.print(f"\n[bold green]Available 2-letter crates:[/bold green]")
        console.print(", ".join(f"[bold]{name}[/bold]" for name in sorted(available)))
    else:
        console.print(
            "\n[bold red]All 676 two-letter combinations are currently taken on crates.io.[/bold red]"
        )


if __name__ == "__main__":
    main()
