//! Built-in Skill Library - Ready-to-use skills from the community

use super::skills::{Skill, SkillImplementation};
// Built-in skill library

/// Get all built-in skills that ship with horcrux
pub fn get_builtin_skills() -> Vec<Skill> {
    vec![
        skill_hackernews_top(),
        skill_weather_check(),
        skill_git_status(),
        skill_system_info(),
        skill_port_scan(),
        skill_json_format(),
        skill_file_backup(),
        skill_dir_size(),
        skill_crypto_price(),
        skill_news_headlines(),
        skill_url_shortener(),
        skill_qr_generator(),
        skill_password_gen(),
        skill_timestamp_convert(),
        skill_base64_convert(),
    ]
}

/// Check if a similar skill already exists
pub fn find_similar_skill<'a>(name: &'a str, description: &'a str, existing: &'a [Skill]) -> Option<&'a Skill> {
    let name_lower = name.to_lowercase();
    let desc_lower = description.to_lowercase();
    
    existing.iter().find(|s| {
        let s_name_lower = s.name.to_lowercase();
        let s_desc_lower = s.description.to_lowercase();
        
        // Check name similarity
        let name_match = s_name_lower.contains(&name_lower) || 
                        name_lower.contains(&s_name_lower) ||
                        name_lower.split('_').any(|part| s_name_lower.contains(part));
        
        // Check description similarity (simple keyword matching)
        let keywords: Vec<_> = desc_lower.split_whitespace()
            .filter(|w| w.len() > 4)
            .collect();
        let keyword_matches = keywords.iter()
            .filter(|&&k| s_desc_lower.contains(k))
            .count();
        let desc_match = keyword_matches >= 2;
        
        name_match || desc_match
    })
}

/// Hacker News top stories
fn skill_hackernews_top() -> Skill {
    Skill {
        name: "hackernews_top".into(),
        description: "Fetch top stories from Hacker News with titles and URLs".into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "count": {
                    "type": "integer",
                    "description": "Number of stories to fetch (default: 5)",
                    "default": 5
                }
            }
        }),
        implementation: SkillImplementation::Shell {
            script: r#"#!/bin/bash
COUNT=${1:-5}
curl -s "https://hacker-news.firebaseio.com/v0/topstories.json" | \
python3 -c "
import sys, json, urllib.request
ids = json.load(sys.stdin)[:$COUNT]
for i, id in enumerate(ids, 1):
    try:
        url = f'https://hacker-news.firebaseio.com/v0/item/{id}.json'
        with urllib.request.urlopen(url) as r:
            data = json.loads(r.read())
            title = data.get('title', 'No title')
            link = data.get('url', f'https://news.ycombinator.com/item?id={id}')
            print(f\"{i}. {title}\")
            print(f\"   {link}\")
            print()
    except Exception as e:
        print(f\"{i}. Error fetching story {id}: {e}\")""#.into(),
            interpreter: "bash".into(),
        },
        created_at: chrono::Utc::now().to_rfc3339(),
        usage_count: 0,
    }
}

/// Weather check
fn skill_weather_check() -> Skill {
    Skill {
        name: "weather_check".into(),
        description: "Get current weather for a city using wttr.in".into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "city": {
                    "type": "string",
                    "description": "City name (e.g., 'London', 'New York', 'Beijing')"
                }
            },
            "required": ["city"]
        }),
        implementation: SkillImplementation::Shell {
            script: "curl -s 'wttr.in/{{city}}?format=3'".into(),
            interpreter: "bash".into(),
        },
        created_at: chrono::Utc::now().to_rfc3339(),
        usage_count: 0,
    }
}

/// Git status pretty
fn skill_git_status() -> Skill {
    Skill {
        name: "git_status".into(),
        description: "Show colorful git status with branch info and file changes".into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {}
        }),
        implementation: SkillImplementation::Shell {
            script: r#"#!/bin/bash
echo "📁 Repository: $(basename $(git rev-parse --show-toplevel 2>/dev/null || pwd))"
echo "🌿 Branch: $(git branch --show-current 2>/dev/null || echo 'Not a git repo')"
echo ""
git -c color.status=always status --short 2>/dev/null || echo "Not a git repository""#.into(),
            interpreter: "bash".into(),
        },
        created_at: chrono::Utc::now().to_rfc3339(),
        usage_count: 0,
    }
}

/// System info
fn skill_system_info() -> Skill {
    Skill {
        name: "system_info".into(),
        description: "Show system information - OS, memory, disk, CPU".into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {}
        }),
        implementation: SkillImplementation::Shell {
            script: r#"#!/bin/bash
echo "🖥️  System Information"
echo "====================="
echo "OS: $(uname -s) $(uname -r)"
echo "Hostname: $(hostname)"
echo "Uptime: $(uptime -p 2>/dev/null || uptime | awk -F',' '{print $1}')"
echo ""
echo "💾 Memory:"
if command -v free &> /dev/null; then
    free -h 2>/dev/null | grep -E "Mem|Swap"
else
    echo "  Memory info not available"
fi
echo ""
echo "💿 Disk Usage:"
df -h / 2>/dev/null | tail -1 | awk '{print "  Used: " $3 " / " $2 " (" $5 ")"}'
echo ""
echo "⚡ CPU:"
if [ -f /proc/cpuinfo ]; then
    grep "model name" /proc/cpuinfo | head -1 | cut -d':' -f2 | xargs
else
    echo "  $(sysctl -n hw.model 2>/dev/null || echo 'CPU info not available')"
fi"#.into(),
            interpreter: "bash".into(),
        },
        created_at: chrono::Utc::now().to_rfc3339(),
        usage_count: 0,
    }
}

/// Port scan
fn skill_port_scan() -> Skill {
    Skill {
        name: "port_scan".into(),
        description: "Scan common ports on a host to check which services are running".into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "host": {
                    "type": "string",
                    "description": "Hostname or IP to scan (e.g., 'localhost', '192.168.1.1')"
                }
            },
            "required": ["host"]
        }),
        implementation: SkillImplementation::Shell {
            script: r#"#!/bin/bash
echo "🔍 Scanning common ports on {{host}}..."
echo ""
for port in 22 80 443 3306 5432 8080 3000 5000; do
    timeout 1 bash -c "echo >/dev/tcp/{{host}}/$port" 2>/dev/null && 
        echo "✅ Port $port: OPEN" || 
        echo "❌ Port $port: closed"
done"#.into(),
            interpreter: "bash".into(),
        },
        created_at: chrono::Utc::now().to_rfc3339(),
        usage_count: 0,
    }
}

/// JSON formatter
fn skill_json_format() -> Skill {
    Skill {
        name: "json_format".into(),
        description: "Format and pretty-print JSON from file or stdin".into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "file": {
                    "type": "string",
                    "description": "Path to JSON file (optional, reads stdin if not provided)"
                }
            }
        }),
        implementation: SkillImplementation::Shell {
            script: r#"#!/bin/bash
if [ -n "{{file}}" ] && [ -f "{{file}}" ]; then
    python3 -m json.tool "{{file}}"
else
    echo "Usage: Provide a valid JSON file path" >&2
    exit 1
fi"#.into(),
            interpreter: "bash".into(),
        },
        created_at: chrono::Utc::now().to_rfc3339(),
        usage_count: 0,
    }
}

/// File backup
fn skill_file_backup() -> Skill {
    Skill {
        name: "file_backup".into(),
        description: "Create a timestamped backup of a file or directory".into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to file or directory to backup"
                }
            },
            "required": ["path"]
        }),
        implementation: SkillImplementation::Shell {
            script: r#"#!/bin/bash
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
BACKUP="{{path}}.backup_$TIMESTAMP"
if [ -e "{{path}}" ]; then
    cp -r "{{path}}" "$BACKUP"
    echo "✅ Backup created: $BACKUP"
else
    echo "❌ Path not found: {{path}}"
    exit 1
fi"#.into(),
            interpreter: "bash".into(),
        },
        created_at: chrono::Utc::now().to_rfc3339(),
        usage_count: 0,
    }
}

/// Directory size
fn skill_dir_size() -> Skill {
    Skill {
        name: "dir_size".into(),
        description: "Show size of directory and its subdirectories (sorted by size)".into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Directory path (default: current directory)"
                }
            }
        }),
        implementation: SkillImplementation::Shell {
            script: r#"#!/bin/bash
DIR="{{path}}"
[ -z "$DIR" ] && DIR="."
echo "📊 Directory sizes in: $DIR"
echo "================================"
du -sh "$DIR"/* 2>/dev/null | sort -rh | head -20 || 
    echo "Error: Could not read directory""#.into(),
            interpreter: "bash".into(),
        },
        created_at: chrono::Utc::now().to_rfc3339(),
        usage_count: 0,
    }
}

/// Crypto price check
fn skill_crypto_price() -> Skill {
    Skill {
        name: "crypto_price".into(),
        description: "Get current cryptocurrency prices (BTC, ETH, etc.)".into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "symbol": {
                    "type": "string",
                    "description": "Crypto symbol (default: bitcoin)",
                    "default": "bitcoin"
                }
            }
        }),
        implementation: SkillImplementation::Shell {
            script: r#"#!/bin/bash
SYM="{{symbol}}"
[ -z "$SYM" ] && SYM="bitcoin"
curl -s "https://api.coingecko.com/api/v3/simple/price?ids=$SYM&vs_currencies=usd&include_24hr_change=true" | \
python3 -c "
import sys, json
data = json.load(sys.stdin)
for coin, info in data.items():
    price = info['usd']
    change = info.get('usd_24h_change', 0)
    change_str = f'{change:+.2f}%' if change else 'N/A'
    print(f\"💰 {coin.capitalize()}: \${price:,.2f} (24h: {change_str})\")""#.into(),
            interpreter: "bash".into(),
        },
        created_at: chrono::Utc::now().to_rfc3339(),
        usage_count: 0,
    }
}

/// News headlines
fn skill_news_headlines() -> Skill {
    Skill {
        name: "news_headlines".into(),
        description: "Fetch latest news headlines (requires newsapi.org key in env)".into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "category": {
                    "type": "string",
                    "enum": ["general", "business", "technology", "science"],
                    "default": "general"
                }
            }
        }),
        implementation: SkillImplementation::Shell {
            script: r#"#!/bin/bash
if [ -z "$NEWS_API_KEY" ]; then
    echo "⚠️  NEWS_API_KEY not set. Get free key at https://newsapi.org"
    exit 1
fi
curl -s "https://newsapi.org/v2/top-headlines?country=us&category={{category}}&apiKey=$NEWS_API_KEY" | \
python3 -c "
import sys, json
data = json.load(sys.stdin)
for i, article in enumerate(data.get('articles', [])[:5], 1):
    print(f\"{i}. {article['title']}\")
    print(f\"   {article['url']}\")
    print()""#.into(),
            interpreter: "bash".into(),
        },
        created_at: chrono::Utc::now().to_rfc3339(),
        usage_count: 0,
    }
}

/// URL shortener
fn skill_url_shortener() -> Skill {
    Skill {
        name: "url_shorten".into(),
        description: "Shorten a URL using is.gd service".into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "Long URL to shorten"
                }
            },
            "required": ["url"]
        }),
        implementation: SkillImplementation::Shell {
            script: "curl -s 'https://is.gd/create.php?format=simple&url={{url}}'".into(),
            interpreter: "bash".into(),
        },
        created_at: chrono::Utc::now().to_rfc3339(),
        usage_count: 0,
    }
}

/// QR code generator
fn skill_qr_generator() -> Skill {
    Skill {
        name: "qr_generate".into(),
        description: "Generate QR code from text/URL (outputs ASCII art)".into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "text": {
                    "type": "string",
                    "description": "Text or URL to encode in QR"
                }
            },
            "required": ["text"]
        }),
        implementation: SkillImplementation::Shell {
            script: "curl -s 'https://api.qrserver.com/v1/create-qr-code/?size=150x150&data={{text}}' -o /tmp/qr.png && echo 'QR code saved to /tmp/qr.png'".into(),
            interpreter: "bash".into(),
        },
        created_at: chrono::Utc::now().to_rfc3339(),
        usage_count: 0,
    }
}

/// Password generator
fn skill_password_gen() -> Skill {
    Skill {
        name: "password_gen".into(),
        description: "Generate strong random password".into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "length": {
                    "type": "integer",
                    "description": "Password length (default: 16)",
                    "default": 16
                }
            }
        }),
        implementation: SkillImplementation::Shell {
            script: "openssl rand -base64 48 | tr -dc 'a-zA-Z0-9!@#$%^&*' | head -c {{length}} && echo".into(),
            interpreter: "bash".into(),
        },
        created_at: chrono::Utc::now().to_rfc3339(),
        usage_count: 0,
    }
}

/// Timestamp converter
fn skill_timestamp_convert() -> Skill {
    Skill {
        name: "timestamp_convert".into(),
        description: "Convert Unix timestamp to human-readable date".into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "timestamp": {
                    "type": "string",
                    "description": "Unix timestamp (seconds since epoch)"
                }
            },
            "required": ["timestamp"]
        }),
        implementation: SkillImplementation::Shell {
            script: "date -d @{{timestamp}} '+%Y-%m-%d %H:%M:%S UTC' 2>/dev/null || date -r {{timestamp}} '+%Y-%m-%d %H:%M:%S UTC'".into(),
            interpreter: "bash".into(),
        },
        created_at: chrono::Utc::now().to_rfc3339(),
        usage_count: 0,
    }
}

/// Base64 converter
fn skill_base64_convert() -> Skill {
    Skill {
        name: "base64_convert".into(),
        description: "Encode/decode Base64".into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "text": {
                    "type": "string",
                    "description": "Text to encode or Base64 to decode"
                },
                "mode": {
                    "type": "string",
                    "enum": ["encode", "decode"],
                    "default": "encode"
                }
            },
            "required": ["text"]
        }),
        implementation: SkillImplementation::Shell {
            script: r#"if [ "{{mode}}" = "decode" ]; then echo "{{text}}" | base64 -d; else echo "{{text}}" | base64; fi"#.into(),
            interpreter: "bash".into(),
        },
        created_at: chrono::Utc::now().to_rfc3339(),
        usage_count: 0,
    }
}
