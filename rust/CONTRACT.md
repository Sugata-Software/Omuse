# Omuse shared implementation contract

This contract defines the active Rust domain boundary. Product-facing names,
commands, environment variables and XDG paths use `Omuse`, `omuse`, `OMUSE_`
and the `omuse` application directories. Legacy names may be accepted as
compatibility aliases, but must not replace the canonical identifiers. The
`.comp` format identifiers and unknown source metadata remain unchanged.

Use this exact core API in model.rs; implementation changes coordinated with parent.
Document: Clone, Debug. pub width:u32, height:u32, name:String, background:[u8;4], layers:Vec<Layer>, metadata:serde_json::Value.
Layer: Clone, Debug. pub id:String,name:String,visible:bool,locked:bool,opacity:f32,blend_mode:String,offset_x:f32,offset_y:f32,rotation:f32,scale_x:f32,scale_y:f32,image:Option<image::RgbaImage>,mask:Option<image::RgbaImage>,children:Vec<Layer>,metadata:serde_json::Value.
Document::new(width:u32,height:u32)->Self (one transparent paint layer); find_layer(&self,id:&str)->Option<&Layer>; find_layer_mut(&mut self,id:&str)->Option<&mut Layer>.
Layer::paint(name:impl Into<String>,width:u32,height:u32)->Self; Layer::group(name:impl Into<String>)->Self.
Layer order bottom-to-top. Rotation degrees. Pixel RGBA straight alpha; layer opacity 0..1, scale 1 identity.
Document I/O: document::open(path:&std::path::Path)->anyhow::Result<Document>; document::save(doc:&Document,path:&std::path::Path)->anyhow::Result<()>; document::import_image(path:&Path)->anyhow::Result<Layer>.
Raster: raster::composite(doc:&Document)->image::RgbaImage; raster::export(doc:&Document,path:&Path)->anyhow::Result<()>.
Editor API created by editor agent and communicated early to parent. Editor owns document and history. No GPUI dependency in domain modules. Validate allocation/file bounds. Preserve unknown source metadata; do not silently destroy unsupported project features.
