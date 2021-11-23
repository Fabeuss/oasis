use crate::entity::error::Error;
use crate::entity::file::{File, FileType};
use crate::entity::request::{CreateDirRequest, GenerateLinkRequest, RenameFileRequest};
use crate::entity::response::FileResponse;
use crate::service::app_state::AppState;
use crate::service::auth::{AuthAdmin, AuthUser};
use crate::service::range::{Range, RangedFile};
use crate::service::track;
use crate::util::{self, file_system};
use anyhow::Result as AnyResult;
use rocket::fs::NamedFile;
use rocket::serde::json::Json;
use rocket::tokio::fs;
use rocket::{Route, State};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub fn route() -> Vec<Route> {
    routes![
        dir_content,
        create_dir,
        change_name,
        delete_file,
        file_content,
        video_track,
        generate_share_link,
        get_share_link,
        search_files
    ]
}

#[get("/dir?<path>")]
async fn dir_content(
    path: Option<&str>,
    _user: AuthUser,
    state: &State<AppState>,
) -> Result<Json<Vec<File>>, Error> {
    let storage = state.get_site()?.storage.clone();
    let target_path = match path {
        Some(dir) => PathBuf::from(&storage).join(&util::parse_encoded_url(dir)?),
        None => PathBuf::from(&storage),
    };

    if !target_path.exists() || !target_path.is_dir() {
        eprintln!("Invalid dir path: {:?}", &target_path);
        return Err(Error::BadRequest);
    }

    let mut dir_iterator = fs::read_dir(target_path).await?;
    let mut content: Vec<File> = Vec::new();
    while let Some(entry) = dir_iterator.next_entry().await? {
        let path = entry.path();
        content.push(File::from_path(&path, false, &storage)?);
    }

    Ok(Json(content))
}

#[post("/dir", data = "<req_body>")]
async fn create_dir(
    state: &State<AppState>,
    req_body: Json<CreateDirRequest>,
    _user: AuthAdmin,
) -> Result<(), Error> {
    let storage = state.get_site()?.storage.clone();
    let parent = util::parse_encoded_url(&req_body.parent)?;
    let parent_path = PathBuf::from(storage).join(&parent);
    let target_path = parent_path.join(&req_body.name);

    if !parent_path.exists() || target_path.exists() {
        return Err(Error::BadRequest);
    }

    Ok(fs::create_dir(target_path).await?)
}

#[get("/file/<path>")]
async fn file_content(
    path: &str,
    _user: AuthUser,
    state: &State<AppState>,
    range_header: Range,
) -> Result<FileResponse, Error> {
    let target_path = get_target_path(state, path).map_err(|e| {
        eprintln!("{}", e);
        return 400;
    })?;

    if !target_path.is_file() {
        return Err(Error::BadRequest);
    }

    let is_text = FileType::get_file_type(&target_path) == FileType::Text;

    match (range_header.range, is_text) {
        (Some(range), _) => {
            let ranged_file = RangedFile::new(range, target_path).await?;
            Ok(FileResponse::Range(ranged_file))
        }
        (None, true) => Ok(FileResponse::Text(
            file_system::read_text_file(target_path).await?,
        )),
        (None, false) => Ok(FileResponse::Binary(NamedFile::open(target_path).await?)),
    }
}

#[put("/file/<path>", data = "<req_body>")]
async fn change_name(
    state: &State<AppState>,
    path: &str,
    req_body: Json<RenameFileRequest>,
    _user: AuthAdmin,
) -> Result<(), Error> {
    let current_file = get_target_path(state, path).map_err(|e| {
        eprintln!("{}", e);
        return 400;
    })?;

    let target_file = current_file.parent().unwrap().join(&req_body.new_name);

    if target_file.exists() {
        return Err(Error::BadRequest);
    }

    fs::rename(current_file, target_file).await?;
    Ok(())
}

#[delete("/file/<path>")]
async fn delete_file(state: &State<AppState>, path: &str, _user: AuthAdmin) -> Result<(), Error> {
    let target_path = get_target_path(state, path).map_err(|e| {
        eprintln!("{}", e);
        return 400;
    })?;

    if target_path.is_file() {
        fs::remove_file(target_path).await?;
    } else {
        fs::remove_dir_all(target_path).await?;
    }

    Ok(())
}

// Temporary solution for track file searching.
// Could remove this function in the future.
#[get("/file/track/<path>")]
async fn video_track(
    path: &str,
    _user: AuthUser,
    state: &State<AppState>,
) -> Result<String, Error> {
    let storage = state.get_site()?.storage.clone();
    let target_path = PathBuf::from(storage).join(&util::parse_encoded_url(path)?);

    let track_str = match track::get_track(target_path).await {
        Ok(str) => str,
        Err(e) => {
            eprintln!("Error when getting track: {}", e);
            return Err(Error::NotFound);
        }
    };

    Ok(track_str)
}

#[post("/file/share", data = "<req_body>")]
async fn generate_share_link(
    state: &State<AppState>,
    req_body: Json<GenerateLinkRequest>,
    _user: AuthUser,
) -> Result<String, Error> {
    let secret = state.get_secret()?;
    let path_encode = urlencoding::encode(&req_body.path);
    let input = format!("expire={}&path={}", req_body.expire, path_encode);
    let hash = util::sha256(&input, &secret);

    Ok(format!("hash={}&{}", hash, input))
}

#[get("/file/share?<path>&<expire>&<hash>")]
async fn get_share_link(
    state: &State<AppState>,
    path: &str,
    expire: i64,
    hash: &str,
    range_header: Range,
) -> Result<FileResponse, Error> {
    let secret = state.get_secret()?;
    let path_encode = urlencoding::encode(path);

    let input = format!("expire={}&path={}", expire, path_encode);
    if hash != util::sha256(&input, &secret) {
        return Err(Error::BadRequest);
    }

    let target_path = get_target_path(state, path).map_err(|e| {
        eprintln!("{}", e);
        return 400;
    })?;

    if !target_path.is_file() {
        return Err(Error::BadRequest);
    }

    match range_header.range {
        Some(range) => {
            let ranged_file = RangedFile::new(range, target_path).await?;
            Ok(FileResponse::Range(ranged_file))
        }
        None => Ok(FileResponse::Binary(NamedFile::open(target_path).await?)),
    }
}

#[get("/file/search?<keywords>")]
async fn search_files(
    state: &State<AppState>,
    keywords: &str,
    _user: AuthUser,
) -> Result<Json<Vec<File>>, Error> {
    let decoded_keywords = util::parse_encoded_url(keywords)?;
    let decoded_keywords_str = decoded_keywords.as_os_str().to_str().unwrap();
    let storage = state.get_site()?.storage.clone();
    let keywords_splits: Vec<&str> = decoded_keywords_str.split("+").collect();
    let results = search_dir_all(&storage, &keywords_splits, &storage)?;

    Ok(Json(results))
}

fn get_target_path(state: &State<AppState>, path: &str) -> AnyResult<PathBuf> {
    let storage = state.get_site()?.storage.clone();
    let target_path = PathBuf::from(&storage).join(&util::parse_encoded_url(path)?);

    if !target_path.exists() {
        return Err(anyhow::anyhow!("Invalid path: {:?}", target_path));
    }

    Ok(target_path)
}

fn search_dir_all(target_path: &str, keywords: &Vec<&str>, storage: &str) -> AnyResult<Vec<File>> {
    let mut results = vec![];

    for entry in WalkDir::new(target_path).follow_links(false).into_iter() {
        let entry = entry?;
        let path = entry.path();
        if contains_all_keywords(path, keywords) {
            let path_buf = PathBuf::from(path);
            let file = File::from_path(&path_buf, true, storage)?;
            results.push(file);
        }
    }

    Ok(results)
}

// All keywords are lower case before sending to backend
fn contains_all_keywords(path: &Path, keywords: &Vec<&str>) -> bool {
    let filename = path.file_name().unwrap().to_str().unwrap().to_lowercase();
    for keyword in keywords.iter() {
        let keyword_low = keyword.to_lowercase();
        if !filename.contains(&keyword_low) {
            return false;
        }

        // Keywords start with "." indicates a filter for file type.
        // Eg. ".js" means searching for js files, and json files should be excluded.
        if keyword_low.starts_with(".") {
            if path.is_dir() {
                return false;
            }

            match path.extension() {
                Some(ext) => {
                    let ext_str = ext.to_str().unwrap();
                    let ext_str_dot = format!(".{}", ext_str);
                    if ext_str_dot != keyword_low {
                        return false;
                    }
                }
                None => return false,
            };
        }
    }

    return true;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_walk_dir() {
        let keywords = vec!["Hello"];
        let storage = "/home/ma/Downloads";
        search_dir_all(&storage, &keywords, storage).expect("Error walking dir");
        assert!(true);
    }
}
