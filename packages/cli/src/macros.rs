macro_rules! register_plugins {
    (
        post: { $($post_id:ident($post_plugin:ty) => $post_initializer:expr),* $(,)? },
        calendar: { $($calendar_id:ident($calendar_plugin:ty) => $calendar_initializer:expr),* $(,)? }
    ) => {
        enum SsufidPluginRegistry {
            $($post_id($post_plugin),)*
            $($calendar_id($calendar_plugin),)*
        }

        impl SsufidPluginRegistry {
            async fn save_run(
                self,
                core: Arc<ssufid::SsufidCore>,
                out_dir: &Path,
                calendar_out_dir: &Path,
                posts_limit: u32,
                calendar_range: ssufid::core::CalendarCrawlRange,
                retry_count: u32,
            ) -> eyre::Result<()> {
                let _ = &calendar_range;
                match self {
                    $(Self::$post_id(plugin) => {
                        crate::save_run(core, out_dir, plugin, posts_limit, retry_count).await
                    },)*
                    $(Self::$calendar_id(plugin) => {
                        crate::save_calendar_run(
                            core,
                            calendar_out_dir,
                            plugin,
                            calendar_range,
                            retry_count,
                        ).await
                    },)*
                }
            }
        }

        fn construct_tasks(
            core: Arc<SsufidCore>,
            out_dir: &Path,
            calendar_out_dir: &Path,
            options: SsufidDaemonOptions,
            calendar_range: ssufid::core::CalendarCrawlRange,
        ) -> Vec<impl std::future::Future<Output = eyre::Result<()>>> {
            let include: Option<HashSet<String>> = options
                .include
                .is_empty()
                .not()
                .then_some(HashSet::from_iter(options.include));
            let exclude: Option<HashSet<String>> = options
                .exclude
                .is_empty()
                .not()
                .then_some(HashSet::from_iter(options.exclude));
            let tasks = [
                $(
                    (
                        <$post_plugin>::IDENTIFIER,
                        SsufidPluginRegistry::$post_id($post_initializer),
                    ),
                )*
                $(
                    (
                        <$calendar_plugin>::IDENTIFIER,
                        SsufidPluginRegistry::$calendar_id($calendar_initializer),
                    ),
                )*
            ];

            if let Some(include) = include {
                tasks
                    .into_iter()
                    .filter_map(|(id, task)| {
                        include.contains(id).then_some(task.save_run(
                            core.clone(),
                            out_dir,
                            calendar_out_dir,
                            options.posts_limit,
                            calendar_range.clone(),
                            options.retry_count,
                        ))
                    })
                    .collect()
            } else if let Some(exclude) = exclude {
                tasks
                    .into_iter()
                    .filter_map(|(id, task)| {
                        exclude.contains(id).not().then_some(task.save_run(
                            core.clone(),
                            out_dir,
                            calendar_out_dir,
                            options.posts_limit,
                            calendar_range.clone(),
                            options.retry_count,
                        ))
                    })
                    .collect()
            } else {
                tasks
                    .into_iter()
                    .map(|(_, task)| {
                        task.save_run(
                            core.clone(),
                            out_dir,
                            calendar_out_dir,
                            options.posts_limit,
                            calendar_range.clone(),
                            options.retry_count,
                        )
                    })
                    .collect()
            }
        }
    };
}

pub(crate) use register_plugins;
