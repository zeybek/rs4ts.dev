---
title: "Date and Time: chrono and the time Crate"
description: "Map JavaScript's single Date to Rust's chrono and time crates, where the type encodes the time zone: parse RFC 3339, format, and do zone-aware arithmetic."
---

## Quick Overview

JavaScript gives you one built-in `Date` object (plus `Intl` for formatting and, increasingly, the `Temporal` proposal), and most Node projects reach for **date-fns**, **Luxon**, or **Day.js** to make it bearable. Rust's standard library deliberately ships only a minimal `std::time` (monotonic clocks and `Duration`, but no calendar), so for real-world dates you pick a crate: **chrono** (the long-standing, feature-rich default) or **time** (a leaner, `const`-friendly alternative). This page shows how a TypeScript developer maps `Date`, ISO parsing, formatting, time zones, and durations onto both, and which one to choose.

> **Note:** `std::time::SystemTime` and `std::time::Instant` exist, but they have no notion of years, months, or time zones — they are for measuring elapsed time, not for calendars. For anything a `Date` does, you want a crate.

---

## TypeScript/JavaScript Example

A typical Node service parses an incoming timestamp, does some arithmetic, formats it for a user in their time zone, and serializes it back to JSON. Here is that flow with the built-in `Date` plus **date-fns** / **date-fns-tz**, which is what most teams actually use:

```typescript
// npm install date-fns date-fns-tz
import { addHours, differenceInHours, format } from "date-fns";
import { formatInTimeZone } from "date-fns-tz";

interface Event {
  name: string;
  startsAt: Date;
}

function parseEvent(raw: string): Event {
  const data = JSON.parse(raw); // `any` — no type checking
  return { name: data.name, startsAt: new Date(data.startsAt) };
}

const event = parseEvent(
  '{ "name": "launch", "startsAt": "2026-06-02T14:30:00Z" }',
);

// Arithmetic
const later = addHours(event.startsAt, 48);
console.log(differenceInHours(later, event.startsAt)); // 48

// Formatting in a specific zone
console.log(formatInTimeZone(event.startsAt, "America/New_York", "yyyy-MM-dd HH:mm zzz"));
// 2026-06-02 10:30 EDT

// Back to JSON — Date serializes to an ISO 8601 string automatically
console.log(JSON.stringify(event));
// {"name":"launch","startsAt":"2026-06-02T14:30:00.000Z"}
```

Three things about `Date` that shape what Rust does differently:

- **`Date` is a single type that means "an instant in UTC".** It stores milliseconds since the Unix epoch. There is *no* separate "date without a time" or "time without a zone" type — `new Date("2026-06-02")` silently invents a midnight UTC instant.
- **Parsing is lenient and quietly fails.** `new Date("not a date")` returns an `Invalid Date` object, not an exception. You only find out when `getTime()` returns `NaN`.
- **Time-zone handling is bolted on.** The core `Date` only knows UTC and the host's local zone; named zones like `America/New_York` require `Intl` or a library.

---

## Rust Equivalent

Here is the same flow with **chrono**, the most widely used date crate. Add it with the `serde` feature so it integrates with JSON, plus **chrono-tz** for the IANA time-zone database (the `America/New_York` data that JavaScript gets from the OS via `Intl`):

```toml
# Cargo.toml — or run:
#   cargo add chrono --features serde
#   cargo add chrono-tz
#   cargo add serde --features derive
#   cargo add serde_json
[dependencies]
chrono = { version = "0.4", features = ["serde"] }
chrono-tz = "0.10"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

```rust
use chrono::{DateTime, Utc};
use chrono_tz::America::New_York;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
struct Event {
    name: String,
    // chrono's serde support parses/renders this as an RFC 3339 string.
    starts_at: DateTime<Utc>,
}

fn main() {
    let raw = r#"{ "name": "launch", "starts_at": "2026-06-02T14:30:00Z" }"#;
    let event: Event = serde_json::from_str(raw).unwrap();

    // Arithmetic with a typed Duration.
    let later = event.starts_at + chrono::Duration::hours(48);
    let diff = later - event.starts_at;
    println!("{}", diff.num_hours()); // 48

    // Formatting in a named time zone.
    let ny = event.starts_at.with_timezone(&New_York);
    println!("{}", ny.format("%Y-%m-%d %H:%M %Z")); // 2026-06-02 10:30 EDT

    // Back to JSON — chrono serializes DateTime<Utc> as RFC 3339.
    println!("{}", serde_json::to_string(&event).unwrap());
    // {"name":"launch","starts_at":"2026-06-02T14:30:00Z"}
}
```

Running this prints:

```text
48
2026-06-02 10:30 EDT
{"name":"launch","starts_at":"2026-06-02T14:30:00Z"}
```

The headline difference: in Rust the **type encodes the zone awareness**. `DateTime<Utc>` is a timestamp that knows it is in UTC; `DateTime<New_York>` knows it is in New York; `NaiveDateTime` knows it has *no* zone at all. The compiler will not let you mix them up, which is exactly the class of bug `Date` invites.

---

## Detailed Explanation

### The core chrono types

chrono splits what JavaScript crams into one `Date` into several types, and the split is the whole point:

| chrono type | Meaning | JavaScript analogue |
| --- | --- | --- |
| `DateTime<Utc>` | An instant, fixed in UTC | `Date` (its true semantics) |
| `DateTime<Local>` | An instant in the host's local zone | `Date` displayed locally |
| `DateTime<Tz>` (chrono-tz) | An instant in a named IANA zone | `Date` + `Intl`/Luxon |
| `DateTime<FixedOffset>` | An instant at a fixed `±HH:MM` offset | the offset in an ISO string |
| `NaiveDateTime` | A wall-clock date+time with **no** zone | a `Date` you forgot to zone |
| `NaiveDate` | A calendar date, no time | date-only string |
| `NaiveTime` | A time of day, no date | — |

> **Tip:** "Naive" here is a term of art, not a judgement. It means "carries no time-zone information." A `NaiveDateTime` of `2026-06-02 14:30` is the literal clock reading; it is *not* an instant until you attach a zone.

### Getting "now"

```rust
use chrono::{Utc, Local, DateTime};

fn main() {
    let now_utc: DateTime<Utc> = Utc::now();
    let now_local: DateTime<Local> = Local::now();

    println!("{}", now_utc.format("%Y-%m-%dT%H:%M:%S%.3fZ"));
    println!("{}", now_local.offset()); // e.g. +03:00
}
```

`Utc::now()` is the `Date.now()` equivalent for getting the current instant, except it returns a richly typed value, not a number of milliseconds. Prefer `Utc::now()` in business logic and only convert to `Local`/a named zone at the edges (when you display to a user).

### Parsing

There are two parsing paths, and choosing the right one is the most common stumbling block:

```rust
use chrono::{DateTime, Utc, NaiveDateTime};

fn main() {
    // 1. An RFC 3339 / ISO 8601 string *with* an offset → DateTime<Utc>.
    let aware: DateTime<Utc> = "2026-06-02T14:30:00Z".parse().unwrap();
    println!("{aware}"); // 2026-06-02 14:30:00 UTC

    // 2. A string *without* an offset → NaiveDateTime, with an explicit format.
    let naive = NaiveDateTime::parse_from_str(
        "2026-06-02 14:30:00",
        "%Y-%m-%d %H:%M:%S",
    ).unwrap();
    println!("{naive}"); // 2026-06-02 14:30:00
}
```

The `.parse()` on the first line uses chrono's `FromStr`, which expects a real RFC 3339 timestamp (an offset is required). For everything else — log lines, CSV columns, custom formats — you use `parse_from_str` with a [strftime-style](https://docs.rs/chrono/latest/chrono/format/strftime/index.html) format string. Unlike `new Date(...)`, parsing returns a `Result` you must handle; there is no silent `Invalid Date`.

### Formatting

`format()` takes the same `strftime` specifiers as parsing and returns a lazy formatter you turn into a `String`:

```rust
use chrono::{TimeZone, Utc};

fn main() {
    let dt = Utc.with_ymd_and_hms(2026, 6, 2, 14, 30, 0).unwrap();
    println!("{}", dt.format("%A, %B %-d, %Y at %H:%M"));
    // Tuesday, June 2, 2026 at 14:30
}
```

Common specifiers: `%Y` 4-digit year, `%m` zero-padded month, `%d` zero-padded day, `%-d` non-padded day, `%H:%M:%S` 24-hour time, `%A`/`%B` full weekday/month name, `%Z` zone abbreviation, `%:z` `+HH:MM` offset, `%.3f` milliseconds.

### Components

The `Datelike` and `Timelike` traits give you the getters:

```rust
use chrono::{TimeZone, Utc, Datelike, Timelike};

fn main() {
    let dt = Utc.with_ymd_and_hms(2026, 6, 2, 14, 30, 0).unwrap();
    println!(
        "year={} month={} day={} hour={} weekday={:?}",
        dt.year(), dt.month(), dt.day(), dt.hour(), dt.weekday(),
    );
    // year=2026 month=6 day=2 hour=14 weekday=Tue
}
```

> **Note:** `month()` and `day()` are **1-based** here, matching human intuition: a welcome contrast to JavaScript's notorious `getMonth()`, which is 0-based (January is `0`).

### Durations and arithmetic

chrono re-exports `TimeDelta` under the familiar name `Duration`. You add and subtract it directly:

```rust
use chrono::{TimeZone, Utc, Duration};

fn main() {
    let dt = Utc.with_ymd_and_hms(2026, 6, 2, 14, 30, 0).unwrap();
    let later = dt + Duration::hours(48) + Duration::minutes(30);
    let diff = later - dt;
    println!("{}", diff.num_hours()); // 48
}
```

For calendar-aware steps (where "one month" is not a fixed number of seconds), use `checked_add_months(Months::new(n))` and `checked_add_days(Days::new(n))`, which return an `Option` because they can land on an impossible date (Jan 31 + 1 month clamps to the end of February).

### Time zones

`with_timezone` converts an instant from one zone's view to another's *without changing the instant* — exactly like Luxon's `setZone`:

```rust
use chrono::{TimeZone, Utc};
use chrono_tz::{America::New_York, Asia::Tokyo};

fn main() {
    let dt = Utc.with_ymd_and_hms(2026, 6, 2, 14, 30, 0).unwrap();
    println!("{}", dt.with_timezone(&New_York).format("%H:%M %Z")); // 10:30 EDT
    println!("{}", dt.with_timezone(&Tokyo).format("%H:%M %Z"));    // 23:30 JST
}
```

chrono-tz bundles the full IANA database, so the right offset (and the right historical DST rules) is applied automatically.

---

## Key Differences

| Concept | TypeScript / JavaScript | Rust (chrono) |
| --- | --- | --- |
| Core type | One `Date` (= UTC instant) | Many types; zone is in the type |
| Date without time | No such type | `NaiveDate` |
| Bad input | `Invalid Date` (silent `NaN`) | `Result::Err` (must handle) |
| Month numbering | 0-based (`getMonth()`) | 1-based (`month()`) |
| Named zones | `Intl` / Luxon / date-fns-tz | chrono-tz (`DateTime<Tz>`) |
| Duration | a `number` of ms, or a library object | typed `Duration` (`TimeDelta`) |
| JSON | `Date` → ISO string via `JSON.stringify` | `DateTime<Utc>` ↔ RFC 3339 via serde |
| Impossible local time | silently shifts | `LocalResult::None` you must handle |

The deepest difference is **zone-awareness in the type system**. JavaScript's `Date` is always a UTC instant under the hood but *displays* as local, so "I have a date" tells you nothing about whether a zone was considered. In chrono, a `NaiveDateTime` cannot be compared to a `DateTime<Utc>` at all: the compiler forces you to decide what zone the wall-clock reading belongs to before it becomes a real instant.

---

## Common Pitfalls

### Parsing a zone-less string straight into `DateTime<Utc>`

A TypeScript developer expects `new Date("2026-06-02 14:30:00")` to "just work." The chrono equivalent does not, because that string carries no offset and chrono's `FromStr` for `DateTime<Utc>` requires one:

```rust
use chrono::{DateTime, Utc};

fn main() {
    // panics at runtime: the string has no offset, so RFC 3339 parsing fails.
    let dt: DateTime<Utc> = "2026-06-02 14:30:00".parse().unwrap();
    println!("{dt}");
}
```

The real panic:

```text
thread 'main' panicked at src/main.rs:5:59:
called `Result::unwrap()` on an `Err` value: ParseError(TooShort)
```

The fix is to parse into a `NaiveDateTime` with an explicit format, then attach the zone you *know* the data is in:

```rust
use chrono::{NaiveDateTime, Utc, TimeZone};

fn main() {
    let naive = NaiveDateTime::parse_from_str(
        "2026-06-02 14:30:00", "%Y-%m-%d %H:%M:%S",
    ).unwrap();
    let dt = Utc.from_utc_datetime(&naive); // declare: "this wall clock is UTC"
    println!("{dt}"); // 2026-06-02 14:30:00 UTC
}
```

### Mixing naive and zone-aware types

You cannot subtract a `NaiveDateTime` from a `DateTime<Utc>`, and unlike JavaScript, this is caught at compile time, not as a `NaN`:

```rust
use chrono::{DateTime, Utc, NaiveDateTime};

fn main() {
    let aware: DateTime<Utc> = Utc::now();
    let naive: NaiveDateTime = NaiveDateTime::parse_from_str(
        "2026-06-02 14:30:00", "%Y-%m-%d %H:%M:%S").unwrap();
    // does not compile (error[E0277]): no Sub<NaiveDateTime> for DateTime<Utc>
    let diff = aware - naive;
    println!("{diff}");
}
```

The real compiler error:

```text
error[E0277]: cannot subtract `NaiveDateTime` from `DateTime<Utc>`
 --> src/main.rs:8:22
  |
8 |     let diff = aware - naive;
  |                      ^ no implementation for `DateTime<Utc> - NaiveDateTime`
  |
  = help: the trait `Sub<NaiveDateTime>` is not implemented for `DateTime<Utc>`
```

This error is a *feature*: it forced you to notice the two values are not the same kind of thing. Convert one to match the other (`naive.and_utc()` to make the naive value a `DateTime<Utc>`) before doing arithmetic.

### Forgetting that some local times do not exist (or exist twice)

Because of daylight saving time, a wall-clock reading can be impossible (spring-forward) or ambiguous (fall-back). chrono surfaces this with `LocalResult` instead of silently guessing the way `Date` does:

```rust
use chrono::{TimeZone, LocalResult};
use chrono_tz::America::New_York;

fn main() {
    // 02:30 on 2026-03-08 does not exist in New York (clocks jump 02:00 → 03:00).
    match New_York.with_ymd_and_hms(2026, 3, 8, 2, 30, 0) {
        LocalResult::Single(dt) => println!("single: {dt}"),
        LocalResult::Ambiguous(a, b) => println!("ambiguous: {a} / {b}"),
        LocalResult::None => println!("none: that local time does not exist"),
    }
    // 01:30 on 2026-11-01 happens twice (clocks fall back 02:00 → 01:00).
    match New_York.with_ymd_and_hms(2026, 11, 1, 1, 30, 0) {
        LocalResult::Single(dt) => println!("single: {dt}"),
        LocalResult::Ambiguous(a, b) => println!("ambiguous: {a} / {b}"),
        LocalResult::None => println!("none"),
    }
}
```

Output:

```text
none: that local time does not exist
ambiguous: 2026-11-01 01:30:00 EDT / 2026-11-01 01:30:00 EST
```

Calling `.unwrap()` on a `LocalResult` panics on **both** `None` and `Ambiguous`; it does *not* quietly pick an instant. To choose deliberately without panicking, use the `.single()` helper (returns an `Option`; `None` unless there is exactly one mapping), or `.earliest()` / `.latest()`. For example, `New_York.with_ymd_and_hms(2026, 11, 1, 1, 30, 0).earliest()` returns `Some(2026-11-01 01:30:00 EDT)` (the earlier, EDT reading). Handle both arms explicitly for any user-supplied local time.

### Calling `.unwrap()` on `from_ymd_opt` with an impossible date

`NaiveDate::from_ymd_opt(2026, 2, 30)` returns `None`, not a clamped date. Reaching for `.unwrap()` on user-controlled values turns a validation problem into a panic; match or use `?` instead.

---

## Best Practices

- **Store and compute in UTC; convert to a zone only for display.** Keep `DateTime<Utc>` everywhere in your domain logic and call `with_timezone` at the boundary, the same discipline you would apply with Luxon.
- **Enable the `serde` feature** so timestamps round-trip through JSON as RFC 3339 with zero boilerplate. It is the chrono equivalent of `Date` serializing to an ISO string.
- **Reach for chrono-tz for named zones.** `DateTime<FixedOffset>` only captures `±HH:MM`; it has no DST rules. Use `chrono_tz::Tz` whenever you need real `America/New_York`-style behavior.
- **Prefer `*_opt` and `checked_*` constructors** (`from_ymd_opt`, `with_ymd_and_hms`, `checked_add_months`) and handle the `Option`/`LocalResult` rather than the panicking shortcuts.
- **Pick calendar-aware arithmetic deliberately.** `Duration::days(30)` is 30 × 86,400 seconds; `Months::new(1)` is "the same day next month." They differ across month boundaries and DST; choose the one your domain means.
- **Consider the `time` crate for libraries and `no_std`/WASM targets** (see below); for application code that needs the broadest ecosystem and named time zones, chrono is the safe default.

---

## Real-World Example

A subscription service: parse a sign-up timestamp from JSON, compute the next monthly renewal date (correctly handling that a Jan 31 sign-up renews on the last day of February), and render a "last seen" string the way date-fns' `formatDistanceToNow` would.

```rust
// cargo add chrono --features serde
// cargo add serde --features derive
// cargo add serde_json
use chrono::{DateTime, Utc, Duration, Months};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
struct Subscription {
    user: String,
    started_at: DateTime<Utc>,
}

/// The next renewal instant strictly after `now`, stepping one calendar month
/// at a time so month lengths are respected (Jan 31 → Feb 28, etc.).
fn next_renewal(started_at: DateTime<Utc>, now: DateTime<Utc>) -> DateTime<Utc> {
    let mut end = started_at;
    while end <= now {
        end = end
            .checked_add_months(Months::new(1))
            .expect("date overflow");
    }
    end
}

/// Human-friendly elapsed time, like date-fns `formatDistanceToNow`.
fn time_ago(then: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let secs = (now - then).num_seconds();
    match secs {
        s if s < 60 => "just now".to_string(),
        s if s < 3_600 => format!("{} minutes ago", s / 60),
        s if s < 86_400 => format!("{} hours ago", s / 3_600),
        s => format!("{} days ago", s / 86_400),
    }
}

fn main() {
    let raw = r#"{ "user": "alice", "started_at": "2026-01-31T00:00:00Z" }"#;
    let sub: Subscription = serde_json::from_str(raw).unwrap();

    let now: DateTime<Utc> = "2026-06-02T12:00:00Z".parse().unwrap();
    println!("next renewal: {}", next_renewal(sub.started_at, now).format("%Y-%m-%d"));

    let last_login: DateTime<Utc> = "2026-05-31T09:00:00Z".parse().unwrap();
    println!("last login: {}", time_ago(last_login, now));

    // Demonstrate the fixed-step alternative is *different*:
    let thirty_days = sub.started_at + Duration::days(30);
    println!("started_at + 30 days: {}", thirty_days.format("%Y-%m-%d"));
}
```

Output:

```text
next renewal: 2026-06-28
last login: 2 days ago
started_at + 30 days: 2026-03-02
```

Note how `next_renewal` lands on the 28th (the last valid "31st-ish" day in February cascaded forward), while naive `+ Duration::days(30)` drifts to March 2 — the calendar-vs-fixed-duration distinction in action.

---

## The `time` crate alternative

**time** is the other major option. It is leaner, has no unsafe code, supports `const` construction via macros, and is the dependency many low-level crates pull in. Its formatting/parsing is based on RFC 3339 plus its own `format_description` mini-language rather than `strftime`. Enable the features you use:

```toml
# cargo add time --features formatting,parsing,macros
[dependencies]
time = { version = "0.3", features = ["formatting", "parsing", "macros"] }
```

```rust
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use time::macros::{datetime, format_description};
use time::Duration;

fn main() {
    // Parse / format RFC 3339.
    let dt = OffsetDateTime::parse("2026-06-02T14:30:00Z", &Rfc3339).unwrap();
    println!("{}", dt.format(&Rfc3339).unwrap()); // 2026-06-02T14:30:00Z

    // A custom, compile-checked format description.
    let fmt = format_description!("[year]-[month]-[day] [hour]:[minute]");
    println!("{}", dt.format(&fmt).unwrap()); // 2026-06-02 14:30

    // Compile-time literal — validated by the compiler, zero runtime parsing.
    let launch = datetime!(2026-06-02 14:30:00 UTC);
    let later = launch + Duration::hours(48);
    println!("{later}"); // 2026-06-04 14:30:00.0 +00:00:00
}
```

How to choose:

| Need | Prefer |
| --- | --- |
| Named IANA zones (`America/New_York`) out of the box | **chrono** (+ chrono-tz) |
| Broadest ecosystem / `strftime` familiarity | **chrono** |
| `const` datetimes, leaner deps, no unsafe | **time** |
| A crate that minimizes its own dependencies | **time** |
| WASM / restricted targets | either, but **time** is common |

> **Warning:** Getting the *local* offset is a subtle area. `time`'s `OffsetDateTime::now_local()` can fail (and is disabled on some multi-threaded Unix builds for soundness reasons), returning an `Err` you must handle. chrono's `Local::now()` is more forgiving. If you only ever work in UTC, neither concern applies.

---

## Further Reading

- [chrono on docs.rs](https://docs.rs/chrono/latest/chrono/): full API reference.
- [chrono `strftime` specifiers](https://docs.rs/chrono/latest/chrono/format/strftime/index.html): the format-string cheat sheet.
- [chrono-tz on docs.rs](https://docs.rs/chrono-tz/latest/chrono_tz/) — the IANA time-zone database.
- [time on docs.rs](https://docs.rs/time/latest/time/): the alternative crate's reference.
- [The `time` book](https://time-rs.github.io/book/) — format descriptions and design rationale.
- Related guide sections:
  - [Popular Crates and the npm Packages They Replace](/23-ecosystem/00-popular-crates/): where chrono and time sit in the wider ecosystem.
  - [Section 02: Basic Types](/02-basics/01-types/): why Rust prefers many precise types over one `number`.
  - [Section 24: Tooling](/24-tooling/) — `cargo add`, features, and managing dependencies.

---

## Exercises

### Exercise 1 — Render an instant in a user's time zone

**Difficulty:** Beginner

**Objective:** Practice parsing RFC 3339 and converting to a named zone.

**Instructions:** Write a function `render_in_zone(rfc3339: &str, zone: &str) -> Result<String, String>` that parses an RFC 3339 timestamp, converts it into the named IANA zone, and returns it formatted as `YYYY-MM-DD HH:MM <ZONE>`. Return a descriptive `Err` string for a bad timestamp or an unknown zone. Add `chrono` and `chrono-tz`.

<details>
<summary>Solution</summary>

```rust
// cargo add chrono
// cargo add chrono-tz
use chrono::{DateTime, Utc};
use chrono_tz::Tz;

fn render_in_zone(rfc3339: &str, zone: &str) -> Result<String, String> {
    let utc: DateTime<Utc> = rfc3339
        .parse()
        .map_err(|e| format!("bad timestamp: {e}"))?;
    let tz: Tz = zone.parse().map_err(|_| format!("unknown zone: {zone}"))?;
    Ok(utc.with_timezone(&tz).format("%Y-%m-%d %H:%M %Z").to_string())
}

fn main() {
    println!("{}", render_in_zone("2026-06-02T14:30:00Z", "Europe/Istanbul").unwrap());
    // 2026-06-02 17:30 +03
    println!("{:?}", render_in_zone("2026-06-02T14:30:00Z", "Mars/Olympus"));
    // Err("unknown zone: Mars/Olympus")
}
```

The `Istanbul` zone prints `+03` for `%Z` because Turkey's current zone has no short abbreviation in the database; chrono falls back to the numeric offset, which is the real, correct output.

</details>

### Exercise 2 — Whole days between two dates

**Difficulty:** Beginner

**Objective:** Use `NaiveDate` and date subtraction.

**Instructions:** Write `days_between(a: &str, b: &str) -> Result<i64, chrono::ParseError>` that parses two `YYYY-MM-DD` strings and returns the whole number of days from `a` to `b` (negative if `b` is earlier). Use `NaiveDate`, since no time or zone is involved.

<details>
<summary>Solution</summary>

```rust
// cargo add chrono
use chrono::NaiveDate;

fn days_between(a: &str, b: &str) -> Result<i64, chrono::ParseError> {
    let start = NaiveDate::parse_from_str(a, "%Y-%m-%d")?;
    let end = NaiveDate::parse_from_str(b, "%Y-%m-%d")?;
    Ok((end - start).num_days())
}

fn main() {
    println!("{}", days_between("2026-01-01", "2026-12-25").unwrap()); // 358
}
```

`NaiveDate` is the right type here: a calendar date has no zone, so reaching for `DateTime` would force you to invent one.

</details>

### Exercise 3 — Count business days in a range

**Difficulty:** Intermediate

**Objective:** Combine `Datelike`, `Weekday`, and date iteration.

**Instructions:** Write `business_days(start: NaiveDate, end: NaiveDate) -> u32` that counts the weekdays (Monday–Friday, inclusive of both endpoints) between two dates. Iterate one day at a time and skip Saturdays and Sundays.

<details>
<summary>Solution</summary>

```rust
// cargo add chrono
use chrono::{NaiveDate, Datelike, Weekday, Duration};

fn business_days(start: NaiveDate, end: NaiveDate) -> u32 {
    let mut count = 0;
    let mut day = start;
    while day <= end {
        if !matches!(day.weekday(), Weekday::Sat | Weekday::Sun) {
            count += 1;
        }
        day += Duration::days(1);
    }
    count
}

fn main() {
    let start = NaiveDate::from_ymd_opt(2026, 6, 1).unwrap();  // Monday
    let end = NaiveDate::from_ymd_opt(2026, 6, 14).unwrap();   // Sunday
    println!("business days: {}", business_days(start, end)); // 10
}
```

For production use you would also subtract public holidays — typically from a `HashSet<NaiveDate>` you check inside the loop.

</details>
