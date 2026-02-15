use leptos::prelude::*;

use crate::state::use_app_state;

#[component]
pub fn IdeationPage() -> impl IntoView {
    let state = use_app_state();
    let ideas = state.ideas;
    let (new_title, set_new_title) = signal(String::new());
    let (new_desc, set_new_desc) = signal(String::new());

    view! {
        <div class="page-header">
            <h2>"Ideation"</h2>
        </div>

        <div class="section">
            <h3>"New Idea"</h3>
            <div class="idea-form">
                <input
                    type="text"
                    class="form-input"
                    placeholder="Idea title..."
                    prop:value=move || new_title.get()
                    on:input=move |ev| set_new_title.set(event_target_value(&ev))
                />
                <textarea
                    class="form-textarea"
                    placeholder="Describe your idea..."
                    prop:value=move || new_desc.get()
                    on:input=move |ev| set_new_desc.set(event_target_value(&ev))
                ></textarea>
                <button class="action-btn action-start">"Submit Idea"</button>
            </div>
        </div>

        <div class="section">
            <h3>"Ideas"</h3>
            <div class="idea-grid">
                {move || ideas.get().into_iter().map(|idea| {
                    let tags_view = idea.tags.iter().map(|tag| {
                        view! {
                            <span class="tag tag-feature">{tag.clone()}</span>
                        }
                    }).collect::<Vec<_>>();
                    view! {
                        <div class="idea-card">
                            <div class="idea-card-header">
                                <span class="idea-title">{idea.title.clone()}</span>
                                <span class="idea-votes">{format!("{} votes", idea.votes)}</span>
                            </div>
                            <div class="idea-description">{idea.description.clone()}</div>
                            <div class="idea-tags">{tags_view}</div>
                            <div class="idea-actions">
                                <button class="action-btn action-start">"Upvote"</button>
                                <button class="action-btn">"Promote to Bead"</button>
                            </div>
                        </div>
                    }
                }).collect::<Vec<_>>()}
            </div>
        </div>
    }
}
