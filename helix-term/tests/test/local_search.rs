use helix_term::application::Application;

use super::*;

#[tokio::test(flavor = "multi_thread")]
async fn local_search_works_on_scratch_buffer() -> anyhow::Result<()> {
    let mut app = helpers::AppBuilder::new()
        .with_input_text("#[|]#fn outer() {\n    fn inner() {}\n}\n")
        .build()?;

    let assertion = |app: &Application| {
        helpers::assert_status_not_error(&app.editor);
    };

    test_key_sequence(&mut app, Some("<space>l"), Some(&assertion), false).await?;

    Ok(())
}
