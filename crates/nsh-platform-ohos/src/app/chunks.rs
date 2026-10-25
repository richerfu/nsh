//! Split source modules are scoped to the accepted HAP entry's directory.
use std::sync::{Arc, Weak};

use crate::app::{
    runtime::{Dispatcher, Message},
    session::Services,
};
pub(crate) struct ResourceChunks {
    host: Arc<Services>,
    prefix: String,
}
impl ResourceChunks {
    pub(crate) fn new(host: Arc<Services>, entry: &str) -> Arc<Self> {
        let prefix = entry
            .rsplit_once('/')
            .map_or(String::new(), |(p, _)| format!("{p}/"));
        Arc::new(Self { host, prefix })
    }
    pub(crate) fn load(self: &Arc<Self>, id: &str, route: Weak<Dispatcher>) -> bool {
        let id = id.to_owned();
        let loader = self.clone();
        let permit = match self.host.reserve_load() {
            Ok(p) => p,
            Err(e) => {
                if let Some(route) = route.upgrade() {
                    route.send(Message::Chunk(Err((id, e.to_string()))));
                }
                return true;
            }
        };
        let failed_route = route.clone();
        let failed_id = id.clone();
        let result = std::thread::Builder::new()
            .name("nsh-chunk".into())
            .spawn(move || {
                let _permit = permit;
                let result = (|| {
                    crate::app::session::resource_name(&id).map_err(|e| e.to_string())?;
                    if id.contains('/') || !id.ends_with(".js") {
                        return Err("expected a chunk file name".into());
                    }
                    let name = format!("{}{id}", loader.prefix);
                    let bytes = loader.host.resource(&name).map_err(|e| e.to_string())?;
                    let code =
                        String::from_utf8(bytes).map_err(|_| "chunk source must be UTF-8")?;
                    let mut chunk = nsh_engine::Chunk::source(id.clone(), code);
                    if let Ok(bytes) = loader.host.resource(&format!("{name}.map"))
                        && let Ok(map) = String::from_utf8(bytes)
                        && !map.is_empty()
                    {
                        chunk.source_map = Some(map.into());
                    }
                    Ok(chunk)
                })();
                if let Some(route) = route.upgrade() {
                    route.send(Message::Chunk(result.map_err(|e| (id, e))));
                }
            });
        if let Err(error) = result
            && let Some(route) = failed_route.upgrade()
        {
            route.send(Message::Chunk(Err((failed_id, error.to_string()))));
        }
        true
    }
}
