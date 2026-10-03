//! Chroma's `gdscript3.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "gdscript3",
    config: ConfigDef {
        name: "GDScript3",
        aliases: &["gdscript3", "gd3"],
        filenames: &["*.gd"],
        mime_types: &["text/x-gdscript", "application/x-gdscript"],
        analyse: Some(AnalyseDef {
            first: false,
            regexes: &[
                (r"func (_ready|_init|_input|_process|_unhandled_input)", 0.8),
                (r"(extends |class_name |onready |preload|load|setget|func [^_])", 0.4),
                (r"(var|const|enum|export|signal|tool)", 0.2),
            ],
        }),
        ..ConfigDef::EMPTY
    },
    states: &[
        ("builtins", &[
            rule(r"(?<!\.)(instance_from_id|nearest_po2|print_stack|type_exist|rand_range|linear2db|var2bytes|dict2inst|randomize|bytes2var|rand_seed|db2linear|inst2dict|printerr|printraw|decimals|preload|deg2rad|str2var|stepify|var2str|convert|weakref|fposmod|funcref|rad2deg|dectime|printt|is_inf|is_nan|assert|Color8|typeof|ColorN|prints|floor|atan2|yield|randf|print|range|clamp|round|randi|sqrt|tanh|cosh|ceil|ease|acos|load|fmod|lerp|seed|sign|atan|sinh|hash|asin|sin|str|cos|tan|pow|exp|min|abs|log|max)\b").token(T::NameBuiltin),
            rule(r"(?<!\.)(self|false|true|PI|NAN|INF)\b").token(T::NameBuiltinPseudo),
            rule(r"(?<!\.)(Physics2DShapeQueryParameters|PhysicsShapeQueryParameters|Physics2DDirectBodyStateSW|NavigationPolygonInstance|ResourceInteractiveLoader|Physics2DDirectSpaceState|Physics2DShapeQueryResult|Physics2DTestMotionResult|InputEventJoystickButton|InputEventJoystickMotion|Physics2DDirectBodyState|PhysicsDirectBodyStateSW|PhysicsShapeQueryResult|PhysicsDirectSpaceState|SpatialSound2DServerSW|PackedDataContainerRef|NavigationMeshInstance|ResourceImportMetadata|PhysicsDirectBodyState|ConcavePolygonShape2D|CanvasItemShaderGraph|EditorScenePostImport|InputEventScreenTouch|InputEventMouseButton|InputEventMouseMotion|SpatialSound2DServer|AudioStreamOGGVorbis|VisibilityNotifier2D|InputEventScreenDrag|ConvexPolygonShape2D|SpatialSoundServerSW|ParticleAttractor2D|PackedDataContainer|SpatialStreamPlayer|RenderTargetTexture|AnimationTreePlayer|ConcavePolygonShape|InstancePlaceholder|MaterialShaderGraph|AudioStreamPlayback|VisibilityEnabler2D|SpatialSamplePlayer|DampedSpringJoint2D|InterpolatedCamera|ConvexPolygonShape|ConfirmationDialog|SpatialSoundServer|BakedLightInstance|ParallaxBackground|CollisionPolygon2D|CanvasItemMaterial|VisibilityNotifier|EditorImportPlugin|VideoStreamTheora|TouchScreenButton|ResourcePreloader|OccluderPolygon2D|BakedLightSampler|CollisionObject2D|RemoteTransform2D|PolygonPathFinder|StyleBoxImageMask|NavigationPolygon|TranslationServer|MultiMeshInstance|ImmediateGeometry|Physics2DServerSW|ColorPickerButton|VisibilityEnabler|PHashTranslation|RectangleShape2D|DirectionalLight|AnimatedSprite3D|WorldEnvironment|CollisionShape2D|EventStreamChibi|InputEventAction|CollisionPolygon|AudioStreamSpeex|EditorFileDialog|GeometryInstance|Generic6DOFJoint|PacketPeerStream|CanvasItemShader|KinematicBody2D|StyleBoxTexture|PhysicsServerSW|VSplitContainer|CenterContainer|GDFunctionState|AudioStreamOpus|TextureProgress|MarginContainer|CollisionObject|LightOccluder2D|AnimationPlayer|HSplitContainer|ScrollContainer|SoundRoomParams|Physics2DServer|MaterialShader|ShaderMaterial|ViewportSprite|SplitContainer|AudioStreamMPC|VisualInstance|PanelContainer|BackBufferCopy|SamplePlayer2D|CanvasModulate|ResourceLoader|CapsuleShape2D|ReferenceFrame|NavigationMesh|CollisionShape|ConeTwistJoint|ProximityGroup|AnimatedSprite|SegmentShape2D|BoneAttachment|RichTextLabel|CircleShape2D|VBoxContainer|PacketPeerUDP|SpatialPlayer|TextureButton|KinematicBody|SoundPlayer2D|PhysicsServer|ParallaxLayer|InputEventKey|GrooveJoint2D|PhysicsBody2D|FixedMaterial|GridContainer|HBoxContainer|StreamPeerSSL|StyleBoxEmpty|StreamPeerTCP|SampleLibrary|GDNativeClass|AudioServerSW|ResourceSaver|SpriteBase3D|StreamPlayer|AtlasTexture|VisualServer|SamplePlayer|StyleBoxFlat|StaticBody2D|SpriteFrames|MeshDataTool|MeshInstance|Vector3Array|BoxContainer|TabContainer|HButtonArray|LargeTexture|Navigation2D|WindowDialog|EditorScript|EditorPlugin|TextureFrame|AcceptDialog|ImageTexture|CapsuleShape|VehicleWheel|VButtonArray|Vector2Array|InputDefault|OptionButton|PathFollow2D|VehicleBody|ColorPicker|PopupDialog|ProgressBar|CanvasLayer|Translation|Environment|EventPlayer|VideoPlayer|EventStream|VideoStream|ButtonGroup|Particles2D|Patch9Frame|ButtonArray|SurfaceTool|MeshLibrary|PackedScene|PhysicsBody|AudioStream|Performance|StringArray|AudioServer|RigidBody2D|LineShape2D|SliderJoint|SphereShape|ShaderGraph|CheckButton|StreamPeer|FileDialog|PathFollow|SceneState|RoomBounds|Dictionary|VSeparator|PacketPeer|VScrollBar|MenuButton|HTTPClient|PinJoint2D|BakedLight|PlaneShape|InputEvent|BaseButton|HSeparator|HScrollBar|Navigation|PopupPanel|StaticBody|Position2D|Position3D|ToolButton|HingeJoint|CanvasItem|RayShape2D|ColorArray|ConfigFile|TCP_Server|RayCast2D|ColorRamp|SpotLight|RealArray|GraphNode|Container|Reference|PopupMenu|Separator|Polygon2D|MultiMesh|Semaphore|Transform|OmniLight|GraphEdit|Particles|Animation|Marshalls|SceneTree|RigidBody|XMLParser|PathRemap|ScrollBar|Directory|PCKPacker|RawArray|TextEdit|MainLoop|TreeItem|StyleBox|Material|Geometry|Matrix32|Resource|UndoRedo|RayShape|TestCube|ItemList|CheckBox|Camera2D|Skeleton|Sprite3D|Viewport|NodePath|IntArray|BoxShape|PinJoint|InputMap|LineEdit|GDScript|Vector3|TileMap|HSlider|Spatial|SpinBox|World2D|IP_Unix|Curve2D|Curve3D|WeakRef|GridMap|Matrix3|VSlider|CubeMap|Joint2D|Globals|Shape2D|Texture|Control|TileSet|Light2D|FuncRef|Vector2|RayCast|Script|Node2D|Button|BitMap|Sample|Object|String|Shader|Area2D|Slider|Sprite|Thread|Path2D|Camera|Portal|float|Theme|World|YSort|Shape|Joint|Mutex|Tween|RegEx|Label|Rect2|Array|Plane|Light|Range|Color|Input|Popup|Panel|Timer|Image|Area|Quad|bool|AABB|Quat|File|Tabs|Path|Font|Tree|Room|Mesh|Node|RID|int|Nil|IP|OS)\b").token(T::NameException),
        ]),
        ("sqs", &[
            rule(r"'").token(T::LiteralStringSingle).pop(1),
            rule(r"\\\\|\\'|\\\n").token(T::LiteralStringEscape),
            include("strings-single"),
        ]),
        ("stringescape", &[
            rule(r#"\\([\\abfnrtv"\']|\n|N\{.*?\}|u[a-fA-F0-9]{4}|U[a-fA-F0-9]{8}|x[a-fA-F0-9]{2}|[0-7]{1,3})"#).token(T::LiteralStringEscape),
        ]),
        ("classname", &[
            rule(r"[a-zA-Z_]\w*").token(T::NameClass).pop(1),
        ]),
        ("strings-single", &[
            rule(r"%(\(\w+\))?[-#0 +]*([0-9]+|[*])?(\.([0-9]+|[*]))?[hlL]?[E-GXc-giorsux%]").token(T::LiteralStringInterpol),
            rule(r#"[^\\\'"%\n]+"#).token(T::LiteralStringSingle),
            rule(r#"[\'"\\]"#).token(T::LiteralStringSingle),
            rule(r"%").token(T::LiteralStringSingle),
        ]),
        ("funcname", &[
            rule(r"[a-zA-Z_]\w*").token(T::NameFunction).pop(1),
            rule("").pop(1),
        ]),
        ("numbers", &[
            rule(r"(\d+\.\d*|\d*\.\d+)([eE][+-]?[0-9]+)?j?").token(T::LiteralNumberFloat),
            rule(r"\d+[eE][+-]?[0-9]+j?").token(T::LiteralNumberFloat),
            rule(r"0[xX][a-fA-F0-9]+").token(T::LiteralNumberHex),
            rule(r"\d+j?").token(T::LiteralNumberInteger),
        ]),
        ("tdqs", &[
            rule(r#"""""#).token(T::LiteralStringDouble).pop(1),
            include("strings-double"),
            rule(r"\n").token(T::LiteralStringDouble),
        ]),
        ("name", &[
            rule(r"[a-zA-Z_]\w*").token(T::Name),
        ]),
        ("root", &[
            rule(r"\n").token(T::Text),
            rule(r#"^(\s*)([rRuUbB]{,2})("""(?:.|\n)*?""")"#).groups(&[T::Text, T::LiteralStringAffix, T::LiteralStringDoc]),
            rule(r"^(\s*)([rRuUbB]{,2})('''(?:.|\n)*?''')").groups(&[T::Text, T::LiteralStringAffix, T::LiteralStringDoc]),
            rule(r"[^\S\n]+").token(T::Text),
            rule(r"#.*$").token(T::CommentSingle),
            rule(r"[]{}:(),;[]").token(T::Punctuation),
            rule(r"\\\n").token(T::Text),
            rule(r"\\").token(T::Text),
            rule(r"(in|and|or|not)\b").token(T::OperatorWord),
            rule(r"!=|==|<<|>>|&&|\+=|-=|\*=|/=|%=|&=|\|=|\|\||[-~+/*%=<>&^.!|$]").token(T::Operator),
            include("keywords"),
            rule(r"(def)((?:\s|\\\s)+)").groups(&[T::Keyword, T::Text]).push(&["funcname"]),
            rule(r"(class)((?:\s|\\\s)+)").groups(&[T::Keyword, T::Text]).push(&["classname"]),
            include("builtins"),
            rule(r#"([rR]|[uUbB][rR]|[rR][uUbB])(""")"#).groups(&[T::LiteralStringAffix, T::LiteralStringDouble]).push(&["tdqs"]),
            rule(r"([rR]|[uUbB][rR]|[rR][uUbB])(''')").groups(&[T::LiteralStringAffix, T::LiteralStringSingle]).push(&["tsqs"]),
            rule(r#"([rR]|[uUbB][rR]|[rR][uUbB])(")"#).groups(&[T::LiteralStringAffix, T::LiteralStringDouble]).push(&["dqs"]),
            rule(r"([rR]|[uUbB][rR]|[rR][uUbB])(')").groups(&[T::LiteralStringAffix, T::LiteralStringSingle]).push(&["sqs"]),
            rule(r#"([uUbB]?)(""")"#).groups(&[T::LiteralStringAffix, T::LiteralStringDouble]).combined(&["stringescape", "tdqs"]),
            rule(r"([uUbB]?)(''')").groups(&[T::LiteralStringAffix, T::LiteralStringSingle]).combined(&["stringescape", "tsqs"]),
            rule(r#"([uUbB]?)(")"#).groups(&[T::LiteralStringAffix, T::LiteralStringDouble]).combined(&["stringescape", "dqs"]),
            rule(r"([uUbB]?)(')").groups(&[T::LiteralStringAffix, T::LiteralStringSingle]).combined(&["stringescape", "sqs"]),
            include("name"),
            include("numbers"),
        ]),
        ("keywords", &[
            rule(r"(breakpoint|continue|onready|extends|signal|return|export|static|setget|switch|break|const|while|class|tool|pass|func|case|enum|else|elif|var|for|do|if)\b").token(T::Keyword),
        ]),
        ("dqs", &[
            rule(r#"""#).token(T::LiteralStringDouble).pop(1),
            rule(r#"\\\\|\\"|\\\n"#).token(T::LiteralStringEscape),
            include("strings-double"),
        ]),
        ("tsqs", &[
            rule(r"'''").token(T::LiteralStringSingle).pop(1),
            include("strings-single"),
            rule(r"\n").token(T::LiteralStringSingle),
        ]),
        ("strings-double", &[
            rule(r"%(\(\w+\))?[-#0 +]*([0-9]+|[*])?(\.([0-9]+|[*]))?[hlL]?[E-GXc-giorsux%]").token(T::LiteralStringInterpol),
            rule(r#"[^\\\'"%\n]+"#).token(T::LiteralStringDouble),
            rule(r#"[\'"\\]"#).token(T::LiteralStringDouble),
            rule(r"%").token(T::LiteralStringDouble),
        ]),
    ],
};
