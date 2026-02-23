use chrono::{Local, NaiveDate};
use serde::Deserialize;

const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const CYAN: &str = "\x1b[36m";
const BLUE: &str = "\x1b[34m";
const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";
const RED: &str = "\x1b[31m";
const RESET: &str = "\x1b[0m";

#[derive(Deserialize)]
struct Response {
    weather: Vec<WeatherEntry>,
}

#[derive(Deserialize)]
struct WeatherEntry {
    timestamp: String,
    temperature: f64,
    precipitation_probability: Option<f64>,
    condition: String,
}

struct DaySummary {
    hi: f64,
    lo: f64,
    max_rp: f64,
    conds: Vec<String>,
    hours: Vec<(String, f64, f64, String)>,
}

fn tc(t: f64) -> &'static str {
    if t < 0.0 { BLUE }
    else if t < 10.0 { CYAN }
    else if t < 20.0 { GREEN }
    else if t < 30.0 { YELLOW }
    else { RED }
}

fn rc(p: f64) -> &'static str {
    if p >= 70.0 { RED }
    else if p >= 40.0 { YELLOW }
    else { DIM }
}

fn icon(cond: &str) -> &'static str {
    match cond {
        "thunderstorm" => "⛈️",
        "rain" => "🌧️",
        "snow" => "❄️",
        "sleet" => "🌨️",
        "hail" => "🧊",
        "fog" => "🌫️",
        "cloudy" => "☁️",
        _ => "☀️",
    }
}

fn pick_icon(conds: &[String]) -> &'static str {
    for c in ["thunderstorm", "rain", "snow", "sleet", "hail", "fog", "cloudy"] {
        if conds.iter().any(|s| s == c) {
            return icon(c);
        }
    }
    icon("dry")
}

fn geocode(city: &str) -> Option<(f64, f64, String)> {
    let url = format!(
        "https://nominatim.openstreetmap.org/search?q={}&format=json&limit=1",
        city.replace(' ', "+")
    );
    let mut resp = ureq::get(&url)
        .header("User-Agent", "weather-cli")
        .call()
        .ok()?;
    let body = resp.body_mut().read_to_string().ok()?;
    let results: Vec<serde_json::Value> = serde_json::from_str(&body).ok()?;
    let first = results.first()?;
    let lat: f64 = first["lat"].as_str()?.parse().ok()?;
    let lon: f64 = first["lon"].as_str()?.parse().ok()?;
    let name = first["display_name"].as_str()?.split(',').next()?.trim().to_string();
    Some((lat, lon, name))
}

fn main() {
    let city = std::env::args().skip(1).collect::<Vec<_>>().join(" ");
    let city = if city.is_empty() {
        eprint!("City: ");
        let mut input = String::new();
        std::io::stdin().read_line(&mut input).unwrap();
        input.trim().to_string()
    } else {
        city
    };

    let (lat, lon, name) = match geocode(&city) {
        Some(v) => v,
        None => {
            eprintln!("Could not find city: {city}");
            return;
        }
    };

    let now = Local::now();
    let today = now.format("%Y-%m-%d").to_string();
    let date_from = now.format("%Y-%m-%dT%H:00").to_string();
    let date_to = (now + chrono::Duration::days(3)).format("%Y-%m-%dT%H:00").to_string();

    let url = format!(
        "https://api.brightsky.dev/weather?lat={lat}&lon={lon}&date={}&last_date={}",
        date_from, date_to
    );

    let body: String = match ureq::get(&url).call() {
        Ok(mut resp) => resp.body_mut().read_to_string().unwrap_or_default(),
        Err(e) => {
            eprintln!("Error: {e}");
            return;
        }
    };

    let resp: Response = match serde_json::from_str(&body) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("JSON error: {e}");
            return;
        }
    };

    let mut days: Vec<(String, DaySummary)> = Vec::new();

    for entry in &resp.weather {
        let day = &entry.timestamp[..10];
        let hour = &entry.timestamp[11..16];
        let t = entry.temperature;
        let rp = entry.precipitation_probability.unwrap_or(0.0);
        let cond = &entry.condition;

        let idx = days.iter().position(|(d, _)| d == day);
        let summary = if let Some(i) = idx {
            &mut days[i].1
        } else {
            days.push((day.to_string(), DaySummary {
                hi: f64::NEG_INFINITY,
                lo: f64::INFINITY,
                max_rp: 0.0,
                conds: Vec::new(),
                hours: Vec::new(),
            }));
            &mut days.last_mut().unwrap().1
        };

        if t > summary.hi { summary.hi = t; }
        if t < summary.lo { summary.lo = t; }
        if rp > summary.max_rp { summary.max_rp = rp; }
        if cond != "dry" && !summary.conds.contains(cond) {
            summary.conds.push(cond.clone());
        }
        if day == today {
            summary.hours.push((hour.to_string(), t, rp, cond.clone()));
        }
    }

    // Cards
    println!("\n  {BOLD}{CYAN}{name}{RESET}");
    println!("  {DIM}                 Temp             Rain{RESET}");
    println!("  {DIM}──────────────────────────────────────{RESET}");
    for (day, d) in &days {
        let ic = pick_icon(&d.conds);
        let label = if day == &today {
            format!("{BOLD}Today{RESET}     ")
        } else {
            let dt = NaiveDate::parse_from_str(day, "%Y-%m-%d").unwrap();
            format!("{:<10}", dt.format("%a %d.%m."))
        };
        println!(
            "  {label} {ic}  {}{:5.1}°{RESET}  …  {}{:5.1}°{RESET}  {}{:3.0}%{RESET}",
            tc(d.lo), d.lo, tc(d.hi), d.hi, rc(d.max_rp), d.max_rp
        );
    }

    // Hourly today
    if let Some((_, d)) = days.iter().find(|(day, _)| day == &today) {
        if !d.hours.is_empty() {
            println!();
            println!("  {DIM}Time         Temp   Rain{RESET}");
            println!("  {DIM}──────────────────────────────────────{RESET}");
            for (hour, t, rp, cond) in &d.hours {
                let ic = icon(cond);
                println!(
                    "  {hour}  {ic}  {}{:5.1}°{RESET}  {}{:3.0}%{RESET}",
                    tc(*t), t, rc(*rp), rp
                );
            }
        }
    }
    println!();
}
