--- a/crates/horizon-client/src/mobs/cem_anim.rs
+++ b/crates/horizon-client/src/mobs/cem_anim.rs
@@ -69,6 +69,8 @@
     "is_holding_item_left" => HoldingLeft, "is_paused" => Paused, "is_hovered" => Hovered,
     "is_player_first_person" => FirstPerson, "is_player_third_person" => ThirdPerson,
     "is_player_third_person_reversed" => ThirdPersonReversed,
+    // EMF's `is_gliding`: Fresh Animations' player reads it.
+    "is_gliding" => Gliding,
 }
 
 /// What every entity of a frame shares, set once per frame by the game.
@@ -558,6 +560,14 @@
         }
         _ => m.extra.pose == 3 || (m.species() == super::Species::Drowned && m.extra.flag),
     };
+    // `LivingEntity.isFallFlying` (`Pose.FALL_FLYING`, id 1), a player's or
+    // mannequin's pose in `avatar_pose`, another mob's in `pose`.
+    let gliding = match &*m.kind.id {
+        "minecraft:player" | "minecraft:mannequin" => {
+            m.extra.avatar_pose == crate::pose::Pose::FallFlying
+        }
+        _ => m.extra.pose == 1,
+    };
     // The arm the current swing moves: the main arm unless the swing named its hand.
     let swinging = m.ai.swinging || m.extra.swing_right_arm.is_some();
     let swing_right = m.extra.swing_right_arm.unwrap_or(!m.main_arm_left());
@@ -652,6 +662,7 @@
             // `isVisuallyCrawling`: the swimming pose out of water.
             Crawling => flag(swim_pose && !m.fluids.in_water),
             Swimming => flag(swim_pose),
+            Gliding => flag(gliding),
             // DATA_LIVING_ENTITY_FLAGS' using bit (the client counts the ticks).
             UsingItem => flag(m.extra.using_item > 0),
             SwingingRight => flag(swinging && swing_right),
