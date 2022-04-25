// SPDX-FileCopyrightText: 2022 Profian Inc. <opensource@profian.com>
// SPDX-License-Identifier: Apache-2.0

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::ops::Deref;

use anyhow::{anyhow, Result};
use pulldown_cmark::{CodeBlockKind, Event, Parser, Tag};

trait CodeBlockKindExt {
    /// Determines whether this code block should be included in output.
    ///
    /// This is based on matching KEY=VALUE filters from `/etc/os-release`.
    fn include(&self, cx: &HashSet<String>, os: &HashMap<String, String>) -> bool;
}

impl CodeBlockKindExt for CodeBlockKind<'_> {
    fn include(&self, cx: &HashSet<String>, os: &HashMap<String, String>) -> bool {
        let param = match self {
            Self::Fenced(k) => match k.split_once(':') {
                Some(("sh", param)) => param,
                _ => return k.deref() == "sh", // Include ```sh blocks
            },
            _ => return false,
        };
        let (c, o) = param.split_once(';').unwrap_or(("", param));
        if !c.is_empty()
            && c.split(',')
                .map(Into::into)
                .collect::<HashSet<_>>()
                .intersection(cx)
                .next()
                .is_none()
        {
            return false;
        }
        o.is_empty()
            || o.split_whitespace()
                .map(|x| x.split_once("=").unwrap())
                .any(|(k, v)| os.get(k).map(String::as_str) == Some(v))
    }
}

/// Returns an iterator over the command lines in code blocks based on OS filters.
fn filter_markdown<'a>(
    cx: &'a HashSet<String>,
    os: impl Read,
    md: &'a str,
) -> Result<impl 'a + Iterator<Item = String>> {
    // Read the distribution variables.
    let os_release = BufReader::new(os)
        .lines()
        .map(|r| match r {
            Ok(line) => line
                .split_once('=')
                .map(|(k, v)| (k.into(), v.into()))
                .ok_or(anyhow!("invalid os-release line: {}", line)),
            Err(e) => Err(anyhow!(e)),
        })
        .collect::<Result<HashMap<_, _>>>()
        .map_err(|e| anyhow!("failed to read os-release: {}", e))?;

    // Filter the command blocks using the filters.
    let mut dump = false;
    Ok(Parser::new(md).filter_map(move |event| match event {
        Event::Start(Tag::CodeBlock(block)) if block.include(cx, &os_release) => {
            dump = true;
            None
        }

        Event::End(Tag::CodeBlock(CodeBlockKind::Fenced(..))) => {
            dump = false;
            None
        }

        Event::Text(text) if dump => Some(text.to_string()),

        _ => None,
    }))
}

fn main() -> Result<()> {
    let mut args = std::env::args();

    let cmd = args.next().unwrap();

    let (md, os) = match (args.next(), args.next()) {
        (Some(md), Some(os)) => (std::fs::read_to_string(md)?, os),
        _ => {
            eprintln!("Usage: {} <markdown> <os-release> [<context>]", cmd);
            std::process::exit(1);
        }
    };

    let cx = args
        .next()
        .map(|s| s.split(',').map(Into::into).collect())
        .unwrap_or_default();

    for cmd in filter_markdown(&cx, File::open(os)?, &md)? {
        print!("{}", cmd);
    }

    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;

    const OS_RELEASE: &str = r#"PRETTY_NAME="Debian GNU/Linux 11 (bullseye)"
NAME="Debian GNU/Linux"
VERSION_ID="11"
VERSION="11 (bullseye)"
VERSION_CODENAME=bullseye
ID=debian
HOME_URL="https://www.debian.org/"
SUPPORT_URL="https://www.debian.org/support"
BUG_REPORT_URL="https://bugs.debian.org/"
"#;

    const MARKDOWN: &str = r#"# Welcome!

Welcome to Enarx.

# Getting Started

## Install Dependencies
### Fedora

```sh:ID=fedora
echo fedora
```

### Debian or Debian-like (e.g. Ubuntu)

```sh:ID=debian ID_LIKE=debian
echo debian
```

## Git or SEV

```sh:git,sev;
echo git or sev
```

## Not Git

```sh:notgit; ID=debian
echo notgit
```

## Git on Debian or Fedora

```sh:git; ID=debian ID=fedora
echo git on debian or fedora
```

## Build Enarx

```sh
echo enarx
```
"#;

    #[test]
    fn test() {
        let mut os = OS_RELEASE.as_bytes();

        assert_eq!(
            filter_markdown(
                &{
                    let mut cx = HashSet::new();
                    cx.insert("git".into());
                    cx
                },
                &mut os,
                MARKDOWN
            )
            .unwrap()
            .collect::<String>(),
            r#"echo debian
echo git or sev
echo git on debian or fedora
echo enarx
"#
        );
    }
}
