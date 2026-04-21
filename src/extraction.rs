use std::collections::HashMap;
use scraper::{Html, Selector};
use crate::app::ExtractedEntity;

pub fn extract_entities(html: &str) -> Vec<ExtractedEntity> {
    let document = Html::parse_document(html);
    let mut entities = Vec::new();

    // 1. Links
    let a_selector = Selector::parse("a[href]").unwrap();
    for element in document.select(&a_selector) {
        if let Some(href) = element.value().attr("href") {
            if !href.starts_with('#') && !href.is_empty() {
                let text = element.text().collect::<Vec<_>>().join(" ").trim().to_string();
                let label = if text.is_empty() { href.to_string() } else { text };
                entities.push(ExtractedEntity {
                    kind: "link".to_string(),
                    value: href.to_string(),
                    label,
                    metadata: HashMap::new(),
                });
            }
        }
    }

    // 2. Images
    let img_selector = Selector::parse("img[src]").unwrap();
    for element in document.select(&img_selector) {
        if let Some(src) = element.value().attr("src") {
            let alt = element.value().attr("alt").unwrap_or("").to_string();
            let label = if alt.is_empty() { src.to_string() } else { alt };
            entities.push(ExtractedEntity {
                kind: "image".to_string(),
                value: src.to_string(),
                label,
                metadata: HashMap::new(),
            });
        }
    }

    // 3. Headings
    for i in 1..=6 {
        let h_selector = Selector::parse(&format!("h{}", i)).unwrap();
        for element in document.select(&h_selector) {
            let text = element.text().collect::<Vec<_>>().join(" ").trim().to_string();
            if !text.is_empty() {
                entities.push(ExtractedEntity {
                    kind: format!("h{}", i),
                    value: text.clone(),
                    label: text,
                    metadata: HashMap::new(),
                });
            }
        }
    }

    // 4. Forms
    let form_selector = Selector::parse("form").unwrap();
    for element in document.select(&form_selector) {
        let action = element.value().attr("action").unwrap_or("").to_string();
        let method = element.value().attr("method").unwrap_or("GET").to_string();
        entities.push(ExtractedEntity {
            kind: "form".to_string(),
            value: action.clone(),
            label: format!("{} [{}]", action, method),
            metadata: HashMap::from([
                ("method".to_string(), method),
            ]),
        });
    }

    // 5. Metadata
    let meta_selector = Selector::parse("meta").unwrap();
    for element in document.select(&meta_selector) {
        let name = element.value().attr("name")
            .or_else(|| element.value().attr("property"))
            .unwrap_or("");
        let content = element.value().attr("content").unwrap_or("");
        
        if !name.is_empty() && !content.is_empty() {
            entities.push(ExtractedEntity {
                kind: "metadata".to_string(),
                value: content.to_string(),
                label: format!("{}: {}", name, content),
                metadata: HashMap::new(),
            });
        }
    }

    entities
}
