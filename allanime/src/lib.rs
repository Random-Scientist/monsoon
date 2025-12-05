use eyre::OptionExt;
use serde::{Deserialize, Serialize};
use tokio::sync::OnceCell;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AllanimeHosterName {
    Default,
    Ok,
    Mp4,
    Ac,
    Ak,
    Kir,
    Rab,
    #[serde(rename = "Luf-mp4")]
    LufMp4,
    #[serde(rename = "Si-Hls")]
    SiHls,
    #[serde(rename = "S-mp4")]
    SMp4,
    #[serde(rename = "Ac-Hls")]
    AcHls,
    #[serde(rename = "Uv-mp4")]
    UvMp4,
    #[serde(rename = "Pn-Hls")]
    PnHls,
    // vidstreaming.io
    #[serde(rename = "Vid-mp4")]
    VidMp4,
    #[serde(rename = "Yt-mp4")]
    YtMp4,
    #[serde(untagged)]
    Other(String),
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AllanimeType {
    Player,
    IFrame,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceUrl {
    source_url: Box<str>,
    source_name: AllanimeHosterName,
    #[serde(rename = "type")]
    source_type: Option<AllanimeType>,
    priority: f64,
}
static IFRAME_HEAD: OnceCell<String> = OnceCell::const_new();

pub async fn query_allanime(query: String, episode: u32) -> eyre::Result<Vec<String>> {
    let c = reqwest::Client::new();
    let body = serde_json::json!({
    "variables": {
      "search": {
        "query": query,
        "allowAdult": false,
        "allowUnknown": false,
      },
      "limit": 26,
      "page": 1,
      "translationType": "sub",
      "countryOrigin": "ALL",
    },
    "query":
        "query(
            $search:SearchInput
            $limit:Int
            $page:Int
            $translationType:VaildTranslationTypeEnumType
            $countryOrigin:VaildCountryOriginEnumType
        ) {
            shows(
                search: $search
                limit: $limit
                page: $page
                translationType: $translationType
                countryOrigin: $countryOrigin
            ) {
                pageInfo {
                    total
                }
                edges {
                    _id
                    name
                    thumbnail
                    englishName
                    nativeName
                    slugTime
                }
            }
        }",
      });
    let br = c
        .post("https://api.allanime.day/api")
        .header("Content-Type", "application/json; charset=utf-8")
        .header("Referer", "https://api.allanime.day/")
        .body(body.to_string())
        .send()
        .await?
        .bytes()
        .await?;
    let v = serde_json::from_slice::<serde_json::Value>(&br)?;
    println!("{v:#?}");
    let showid = v["data"]["shows"]["edges"]
        .as_array()
        .unwrap()
        .first()
        .unwrap()["_id"]
        .as_str()
        .unwrap();
    let body = serde_json::json!(
        {
          "variables": {
            "showId": showid,
            "translationType": "sub",
            "episodeString": episode.to_string(),
          },
          "query":
              "query(
                $showId:String!,
                $translationType: VaildTranslationTypeEnumType!,
                $episodeString: String!
              ) {
                episode(
                showId: $showId
                translationType: $translationType
                episodeString: $episodeString
                ) {
                    sourceUrls
                }
              }",
        }
    );
    let br = c
        .post("https://api.allanime.day/api")
        .header("Content-Type", "application/json; charset=utf-8")
        .header("Referer", "https://api.allanime.day/")
        .body(body.to_string())
        .send()
        .await?
        .bytes()
        .await?;
    let v = serde_json::from_slice::<serde_json::Value>(&br)?;

    for source in v["data"]["episode"]["sourceUrls"].as_array().unwrap() {
        let url: SourceUrl = serde_json::from_value(source.clone())?;
        let decrypted = quote_unquote_decrypt(&url.source_url);
        let real_url: &str = decrypted.as_deref().unwrap_or(&url.source_url);
        match &url.source_name {
            _ if real_url.starts_with("/apivtwo") => {
                // handle internal source
            }
            _ if url.source_type == Some(AllanimeType::Player) => {
                let head = IFRAME_HEAD
                    .get_or_try_init::<eyre::Report, _, _>(|| async move {
                        let resp = reqwest::get("https://allanime.ai/getVersion").await?;
                        let b = resp.bytes().await?;
                        dbg!(str::from_utf8(&b));
                        // .json::<serde_json::Value>()
                        // .await?
                        // .get("episodeIframeHead")
                        // .ok_or_eyre("allanime iframe head not present")
                        // .map(ToString::to_string)
                        todo!()
                    })
                    .await?;
                dbg!(head);
            }
            AllanimeHosterName::Other(_) => {}
            _ => {}
        }

        dbg!(real_url, &url.source_name, &url.source_type);
        println!();
    }

    println!("{v:#?}");

    todo!()
}

pub fn quote_unquote_decrypt(s: &str) -> Option<String> {
    if s.starts_with('-')
        && let Some(s) = s.split('-').next_back()
        && s.is_ascii()
        && s.len() % 2 == 0
    {
        Some(
            s.as_bytes()
                .chunks(2)
                .map(|v| (u8::from_str_radix(str::from_utf8(v).unwrap(), 16).unwrap() ^ 56) as char)
                .collect(),
        )
    } else {
        None
    }
}

#[cfg(test)]
mod test {
    use crate::query_allanime;

    #[tokio::test]
    async fn test() {
        query_allanime("bloom into you".into(), 10).await.unwrap();
    }
}
