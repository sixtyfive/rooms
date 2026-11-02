use crate::config::Config;
use crate::err;
use crate::model::{tidy, Data, Entity};

const TEMPLATE: &str = include_str!("../../shared/template.j2");
const CLIMATE_EXTRA: &str = include_str!("../../shared/climate_extra.j2");

fn label_for<'a>(cfg: &'a Config, domain: &str) -> &'a str {
    match domain {
        "switch" => &cfg.switch_label,
        "climate" => &cfg.climate_label,
        _ => &cfg.light_label,
    }
    .trim()
}

fn render_template(cfg: &Config, domain: &str) -> Result<String, String> {
    let label = label_for(cfg, domain);
    let allowed = if label.is_empty() {
        "none".to_string()
    } else {
        format!("label_entities({})", serde_json::to_string(label).map_err(err)?)
    };
    let extra = if domain == "climate" { CLIMATE_EXTRA.trim() } else { "" };
    Ok(TEMPLATE
        .trim()
        .replace("DOMAIN", domain)
        .replace("EXTRA", extra)
        .replace("ALLOWED", &allowed))
}

pub async fn fetch(cfg: &Config, domain: &str) -> Result<Vec<Entity>, String> {
    let template = render_template(cfg, domain)?;
    let text = reqwest::Client::new()
        .post(format!("{}/api/template", cfg.url.trim_end_matches('/')))
        .bearer_auth(&cfg.token)
        .json(&serde_json::json!({ "template": template }))
        .send()
        .await
        .map_err(err)?
        .error_for_status()
        .map_err(err)?
        .text()
        .await
        .map_err(err)?;
    let mut entities: Vec<Entity> = serde_json::from_str(&text).map_err(err)?;
    tidy(&mut entities);
    Ok(entities)
}

pub async fn fetch_all(cfg: &Config) -> Result<Data, String> {
    let (lights, switches, climate) = tokio::try_join!(
        fetch(cfg, "light"),
        fetch(cfg, "switch"),
        fetch(cfg, "climate")
    )?;
    Ok(Data { lights, switches, climate })
}

async fn call(cfg: &Config, path: &str, body: serde_json::Value) -> Result<(), String> {
    reqwest::Client::new()
        .post(format!("{}/api/services/{path}", cfg.url.trim_end_matches('/')))
        .bearer_auth(&cfg.token)
        .json(&body)
        .send()
        .await
        .map_err(err)?
        .error_for_status()
        .map_err(err)?;
    Ok(())
}

async fn settle(cfg: &Config, entity_id: &str, done: impl Fn(&Entity) -> bool) -> Result<Data, String> {
    let mut last = fetch_all(cfg).await?;
    for _ in 0..10 {
        if last.iter().any(|e| e.id == entity_id && done(e)) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        last = fetch_all(cfg).await?;
    }
    Ok(last)
}

pub async fn set_state(cfg: &Config, entity_id: &str, on: bool) -> Result<Data, String> {
    let service = if on { "turn_on" } else { "turn_off" };
    call(cfg, &format!("homeassistant/{service}"), serde_json::json!({ "entity_id": entity_id })).await?;
    let want = if on { "on" } else { "off" };
    settle(cfg, entity_id, |e| e.state == want).await
}

pub async fn set_temperature(cfg: &Config, entity_id: &str, temperature: f64) -> Result<Data, String> {
    call(
        cfg,
        "climate/set_temperature",
        serde_json::json!({ "entity_id": entity_id, "temperature": temperature }),
    )
    .await?;
    settle(cfg, entity_id, |e| e.target.is_some_and(|t| (t - temperature).abs() < 0.05)).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_substitution() {
        let cfg = Config { switch_label: "panel".into(), ..Default::default() };
        let t = render_template(&cfg, "switch").unwrap();
        assert!(t.contains("states.switch"));
        assert!(t.contains(r#"label_entities("panel")"#));
        assert!(!t.contains("EXTRA") && !t.contains("DOMAIN") && !t.contains("ALLOWED"));
        assert!(!t.contains("current_temperature"));

        let t = render_template(&cfg, "climate").unwrap();
        assert!(t.contains("set allowed = none"));
        assert!(t.contains("current_temperature"));
    }
}
