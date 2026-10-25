//! Explicit --ui-smoke fixture only. Requires an isolated managed model pack,
//! trusted runtime override and an explicitly supplied licensed photo. No download.
use super::*;
use anyhow::{Context as _, ensure};
impl EditorView {
    pub(in crate::ui) fn start_native_guided_probe(
        &mut self,
        dir: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.recovery = Recovery::at(dir.join("recovery"));
        self.dialog = Dialog::None;
        self.path = None;
        cx.spawn_in(window,async move|view,cx|{
            let result=async {
                std::fs::create_dir_all(&dir)?;
                let path=omuse::identity::env_var_os("OMUSE_NATIVE_GUIDED_SAMPLE").context("Set OMUSE_NATIVE_GUIDED_SAMPLE to the explicitly licensed photo fixture")?;
                let path=PathBuf::from(path);
                let metadata=std::fs::symlink_metadata(&path)?;
                ensure!(metadata.is_file()&&!metadata.file_type().is_symlink()&&metadata.len()<=32*1024*1024,"Photo fixture must be a regular file up to32MiB");
                let source=cx.background_executor().spawn(async move {
                    let dimensions=image::image_dimensions(&path)?;
                    ensure!(dimensions.0>0&&dimensions.1>0&&u64::from(dimensions.0)*u64::from(dimensions.1)<=guided_mask::MAX_SOURCE_PIXELS,"Photo fixture exceeds16MP");
                    Ok::<_,anyhow::Error>(image::open(path)?.to_rgba8())
                }).await?;
                let source=Arc::new(source);let original_hash=omuse::asset_library::sha256_hex(source.as_raw());
                ensure!(ManagedPack::installed()?.available(),"The isolated profile must have the verified optional model pack and runtime before this probe; it never downloads them");
                for phase in ["positive","box","cancel"] {
                    let image=source.clone();
                    view.update_in(cx,|this,window,cx|->anyhow::Result<()> {
                        ensure!(!this.guided_mask_has_work(),"A previous worker still owns resources");
                        let mut doc=Document::new(image.width(),image.height());doc.layers[0].name="Licensed guided-mask photo".into();doc.layers[0].image=Some(image.as_ref().clone().into());
                        this.install_opened_content(doc,None);this.path=None;this.live_stamp=None;this.dialog=Dialog::None;this.refresh(cx);this.command("fit",window,cx);
                        if phase=="box"{this.editor.select_rect(1,1,2,2);}
                        this.open_guided_mask(window,cx);ensure!(this.guided_mask_active(),"Open guided mask failed: {}",this.status);Ok(())
                    })??;
                    let deadline=std::time::Instant::now()+std::time::Duration::from_secs(120);
                    while view.update(cx,|this,_|this.guided_mask_has_work())? {
                        ensure!(std::time::Instant::now()<deadline,"Source preparation timed out");cx.background_executor().timer(std::time::Duration::from_millis(20)).await;
                    }
                    let before_selection=view.update(cx,|this,_|this.editor.selection.clone())?;
                    view.update(cx,|this,cx|->anyhow::Result<()> {
                        let d=this.guided_mask.draft.as_mut().context("Guided draft disappeared")?;let w=d.review.source.width as f32;let h=d.review.source.height as f32;
                        d.review.add_prompt(if phase=="box"{Prompt::Box{left:w*0.15,top:h*0.08,right:w*0.85,bottom:h*0.95}}else{Prompt::Positive{x:w*0.5,y:h*0.5}})?;
                        this.update_guided_mask(cx);ensure!(this.guided_mask.active.is_some(),"Model did not start: {}",this.status);Ok(())
                    })??;
                    if phase=="cancel" {
                        cx.background_executor().timer(std::time::Duration::from_millis(50)).await;
                        view.update_in(cx,|this,window,cx|{ensure!(this.guided_mask_has_work(),"The inference finished before this cancellation probe");ensure!(this.cancel_guided_mask(window,cx),"Cancel was not consumed");Ok::<(),anyhow::Error>(())})??;
                        while view.update(cx,|this,_|this.guided_mask_has_work())? {ensure!(std::time::Instant::now()<deadline,"Cancelled owner did not exit");cx.background_executor().timer(std::time::Duration::from_millis(20)).await;}
                        view.update(cx,|this,_|{ensure!(!this.guided_mask_active()&&this.editor.undo_depth()==0&&this.editor.document.layers[0].mask.is_none(),"Cancelled job changed the document");ensure!(this.editor.selection==before_selection,"Cancel changed the earlier selection");Ok::<(),anyhow::Error>(())})??;
                        continue;
                    }
                    loop {
                        cx.update(|window,cx|{window.refresh();window.draw(cx).clear(cx);})?;
                        if view.update(cx,|this,_|this.guided_mask_preview_current())? {break;}
                        ensure!(std::time::Instant::now()<deadline,"Guided preview did not become current: {}",view.update(cx,|this,_|this.status.clone())?);
                        cx.background_executor().timer(std::time::Duration::from_millis(20)).await;
                    }
                    let (reviewed,model_scores)=view.update(cx,|this,_|{let d=this.guided_mask.draft.as_ref().unwrap();(d.review.mask_arc().unwrap(),d.review.alternatives.iter().map(|a|a.estimated_iou).collect::<Vec<_>>())})?;
                    reviewed.save(dir.join(format!("{phase}-reviewed-mask.png")))?;
                    std::fs::write(dir.join(format!("{phase}-preview-ready")), b"actual native preview current")?;
                    let inspect=omuse::identity::env_var("OMUSE_NATIVE_GUIDED_INSPECT_SECONDS").ok().and_then(|v|v.parse::<u64>().ok()).unwrap_or(0).min(120);
                    if inspect>0 {cx.background_executor().timer(std::time::Duration::from_secs(inspect)).await;}

                    std::fs::write(dir.join(format!("{phase}-model-ranking.json")),serde_json::to_vec_pretty(&model_scores)?)?;
                    if phase=="positive" {
                        view.update_in(cx,|this,window,cx|->anyhow::Result<()> {
                            let source=this.editor.document.layers[0].image.as_ref().unwrap().as_arc();this.keep_guided_mask(window,cx);
                            ensure!(!this.guided_mask_active()&&this.editor.undo_depth()==1,"Keep must create exactly one Undo");
                            let layer=&this.editor.document.layers[0];ensure!(Arc::ptr_eq(&source,&layer.image.as_ref().unwrap().as_arc()),"Keep replaced source pixels");
                            let mask=layer.mask.as_ref().context("Kept mask missing")?;ensure!(mask.dimensions()==reviewed.dimensions()&&mask.pixels().zip(reviewed.pixels()).all(|(a,b)|a[0]==b[0]),"Kept mask differs from actually painted review");
                            ensure!(this.editor.undo()&&this.editor.document.layers[0].mask.is_none(),"Undo did not restore prior mask");ensure!(this.editor.redo(),"Redo failed");this.refresh(cx);Ok(())
                        })??;
                        let document=view.update(cx,|this,_|this.editor.document.clone())?;let saved=dir.join("guided-photo.omuse");
                        let reopened=cx.background_executor().spawn(async move {document::save(&document,&saved)?;document::open(&saved)}).await?;
                        ensure!(reopened.layers[0].mask.as_ref().context("Reopened mask missing")?.pixels().zip(reviewed.pixels()).all(|(a,b)|a[0]==b[0]),"Save/reopen changed the reviewed mask");
                        ensure!(omuse::asset_library::sha256_hex(reopened.layers[0].image.as_ref().unwrap().as_raw())==original_hash,"Save/reopen changed source pixels");
                    }else {
                        view.update_in(cx,|this,window,cx|this.use_guided_mask_as_selection(window,cx))?;
                        while view.update(cx,|this,_|this.guided_mask_has_work())? {ensure!(std::time::Instant::now()<deadline,"Selection preparation timed out");cx.background_executor().timer(std::time::Duration::from_millis(20)).await;}
                        view.update(cx,|this,_|->anyhow::Result<()> {ensure!(!this.guided_mask_active()&&this.editor.document.layers[0].mask.is_none(),"Use as selection changed artwork");ensure!(this.editor.selection!=before_selection&&this.editor.undo_depth()==1,"Selection must have one Undo");ensure!(this.editor.undo()&&this.editor.selection==before_selection,"Selection Undo did not restore prior selection");Ok(())})??;
                    }
                }
                std::fs::write(dir.join("guided-native-results.json"),serde_json::to_vec_pretty(&serde_json::json!({"status":"passed","scope":"Native licensed-photo positive and box inference, actual-painted preview, exact mask Keep/Undo/Redo/save/reopen, selection Undo, cancelled native job no late commit","source_sha256":original_hash,"qualification_limits":"Quality must be visually reviewed from mask outputs. Default centre/box prompts may be inappropriate for some photos. Cancellation waits for a native job, not proof that ORT Run had begun; separate CPU test proves RunOptions termination. No8/16MP or Windows claim."}))?)?;
                Ok::<(),anyhow::Error>(())
            }.await;
            if let Err(error)=result {let _=std::fs::write(dir.join("guided-native-error.txt"),format!("{error:#}"));eprintln!("Guided native probe failed: {error:#}");}
            let seconds=omuse::identity::env_var("OMUSE_NATIVE_INSPECT_SECONDS").ok().and_then(|v|v.parse::<u64>().ok()).unwrap_or(30).clamp(5,600);
            cx.background_executor().timer(std::time::Duration::from_secs(seconds)).await;let _=cx.update(|_,cx|cx.quit());
        }).detach();
    }
}
