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
    let body = serde_json::json!(
        {
          "variables": {
            "showId": v["data"]["shows"]["edges"].as_array().unwrap().first().unwrap()["_id"].as_str().unwrap(),
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

    println!("{v:#?}");

    todo!()
}

#[cfg(test)]
mod test {
    use crate::query_allanime;

    #[tokio::test]
    async fn test() {
        query_allanime("bloom into you".into(), 10).await.unwrap();
    }
}
