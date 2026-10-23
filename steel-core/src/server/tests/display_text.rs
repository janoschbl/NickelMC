use super::*;

use crate::entity::entities::TextDisplayEntity;
use crate::entity::entities::objects::technical::Display;
use crate::scoreboard::ScoreHolder;

fn load_text(entity: &Arc<TextDisplayEntity>, text: TextComponent) {
    let mut nbt = NbtCompound::new();
    nbt.insert("text", text);
    let mut bytes = Vec::new();
    nbt.write(&mut bytes);
    let borrowed = simdnbt::borrow::read_compound(&mut Cursor::new(bytes.as_slice())).unwrap();
    Arc::clone(entity).load_additional_with_context((&borrowed).into());
}

#[test]
fn display_load_resolves_unpublished_self_scores_and_domain_storage() {
    init_behaviors();
    init_entities();
    let fixture = fresh_test_world_in_domain("display_text", "overworld");
    let world = &fixture.world;
    let runtime = Builder::new_current_thread().enable_all().build().unwrap();
    runtime.block_on(async {
        let domain = ResolvedDomainConfig {
            name: world.domain().to_owned(),
            default_world: world.key.clone(),
            worlds: vec![world.key.clone()],
        };
        let server = test_server_with_worlds(
            domain.name.clone(),
            slice::from_ref(&domain),
            slice::from_ref(world),
            PermissionSubjectIndex::new(),
        )
        .await
        .unwrap();
        let entity = Arc::new(TextDisplayEntity::new(
            &vanilla_entities::TEXT_DISPLAY,
            19,
            DVec3::new(8.0, 65.0, 8.0),
            Arc::downgrade(world),
        ));
        entity.set_custom_name(Some(TextComponent::plain("Unpublished")));
        load_text(&entity, TextComponent::entity("@s", None));
        entity.with_view(|view| {
            assert_eq!(view.text_ref().to_plain(&DisplayResolutor), "Unpublished")
        });

        load_text(
            &entity,
            TextComponent::nbt(
                "CustomName",
                text_components::content::NbtSource::Entity("@s".into()),
                true,
                None,
            ),
        );
        entity.with_view(|view| {
            assert_eq!(view.text_ref().to_plain(&DisplayResolutor), "Unpublished")
        });

        let scoreboard = server.scoreboards.get(world.domain()).unwrap();
        let objective = scoreboard.add_objective("points").unwrap();
        scoreboard
            .set_score(&ScoreHolder::new(entity.scoreboard_name()), &objective, 37)
            .unwrap();
        load_text(&entity, TextComponent::scoreboard("@s", "points"));
        entity.with_view(|view| assert_eq!(view.text_ref().to_plain(&DisplayResolutor), "37"));

        load_text(&entity, TextComponent::scoreboard("*", "points"));
        entity.with_view(|view| assert_eq!(view.text_ref().to_plain(&DisplayResolutor), "37"));

        let mut data = NbtCompound::new();
        data.insert("message", "stored text");
        server.command_storage.get(world.domain()).unwrap().set(
            steel_utils::Identifier::vanilla_static("display_test"),
            data,
        );
        load_text(
            &entity,
            TextComponent::nbt(
                "message",
                text_components::content::NbtSource::storage("minecraft:display_test"),
                false,
                None,
            ),
        );
        entity.with_view(|view| {
            assert_eq!(view.text_ref().to_plain(&DisplayResolutor), "stored text")
        });

        load_text(&entity, TextComponent::entity("@e[", None));
        entity.with_view(|view| {
            assert_eq!(view.text_ref().to_plain(&DisplayResolutor), "stored text")
        });
        assert!(
            world.get_entity_by_id(entity.id()).is_none(),
            "resolution must not publish the loading entity"
        );
    });
}
