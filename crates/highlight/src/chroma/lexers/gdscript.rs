//! Chroma's `gdscript.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "gdscript",
    config: ConfigDef {
        name: "GDScript",
        aliases: &["gdscript", "gd"],
        filenames: &["*.gd"],
        mime_types: &["text/x-gdscript", "application/x-gdscript"],
        priority: 0.1,
        analyse: Some(AnalyseDef {
            first: false,
            regexes: &[
                (r"^export", 0.1),
            ],
        }),
        ..ConfigDef::EMPTY
    },
    states: &[
        ("dqs", &[
            rule(r#"""#).token(T::LiteralStringDouble).pop(1),
            include("strings_double"),
        ]),
        ("tdqs", &[
            rule(r#"""""#).token(T::LiteralStringDouble).pop(1),
            include("strings_double"),
        ]),
        ("keywords", &[
            rule(r"(?<!\w)(PI|TAU|NAN|INF|true|false)\b").token(T::KeywordConstant),
            rule(r"(?<!\w)(is|in|as|not|or|and)\b").token(T::OperatorWord),
            rule(r"(?<!\w)(var|const|enum|signal|static)\b").token(T::KeywordDeclaration),
            rule(r"(?<!\w)(if|elif|else|for|while|match|break|continue|pass|return|breakpoint|await|yield|super)\b").token(T::Keyword),
            rule(r"(?<!\w)(self)\b").token(T::NameBuiltinPseudo),
        ]),
        ("builtin_funcs", &[
            rule(r"(?<!\w)(assert|char|convert|dict_to_inst|get_stack|inst_to_dict|is_instance_of|len|load|preload|print_debug|print_stack|range|type_exists)\b").token(T::NameBuiltin),
            rule(r"(?<!\w)(abs[fi]?|acos|asin|atan2?|bezier_(derivative|interpolate)|bytes_to_var(_with_objects)?|ceil[fi]?|clamp[fi]?|cosh?|cubic_interpolate(_angle)?(_in_time)?|db_to_linear|deg_to_rad|ease|error_string|exp|floor[fi]?|fmod|fposmod|hash|instance_from_id|inverse_lerp|is_equal_approx|is_finite|is_instance(_id)?_valid|is_nan|is_same|is_zero_approx|lerp|lerp_angle|lerpf|linear_to_db|log|max[fi]?|min[fi]?|move_toward|nearest_po2|pingpong|posmod|pow|print|print_rich|print_verbose|printerr|printraw|prints|printt|push_error|push_warning|rad_to_deg|rand_from_seed|randf|randf_range|randfn|randi|randi_range|randomize|remap|rid_allocate_id|rid_from_int64|round[fi]?|seed|sign[fi]?|sinh?|smoothstep|snapped[fi]?|sqrt|step_decimals|str|str_to_var|tanh?|typeof|var_to_bytes(_with_objects)?|var_to_str|weakref|wrap[fi]?)\b").token(T::NameBuiltin),
        ]),
        ("tsqs", &[
            rule(r"'''").token(T::LiteralStringSingle).pop(1),
            include("strings_single"),
        ]),
        ("strings_single", &[
            include("strings"),
            rule(r"\{[^\\\'\n]+\}").token(T::LiteralStringInterpol),
            rule(r"[^\\\'\{%]+").token(T::LiteralStringSingle),
            rule(r"%").token(T::LiteralStringSingle),
            rule(r"{").token(T::LiteralStringSingle),
        ]),
        ("funcname", &[
            rule(r"[a-zA-Z_]\w*").token(T::NameFunction).pop(1),
        ]),
        ("numbers", &[
            rule(r"(\d+\.\d*|\d*\.\d+)([eE][+-]?[0-9]+)?").token(T::LiteralNumberFloat),
            rule(r"\d+[eE][+-]?[0-9]+").token(T::LiteralNumberFloat),
            rule(r"0x[a-fA-F0-9]+").token(T::LiteralNumberHex),
            rule(r"0b[01]+").token(T::LiteralNumberBin),
            rule(r"\d+").token(T::LiteralNumberInteger),
        ]),
        ("sqs", &[
            rule(r"'").token(T::LiteralStringSingle).pop(1),
            include("strings_single"),
        ]),
        ("classname", &[
            rule(r"[a-zA-Z_]\w*").token(T::NameClass).pop(1),
        ]),
        ("annotations", &[
            rule(r"^\s*@export(_category|_color_no_alpha|_dir|_enum|_exp_easing|_file|_flags((_2d|_3d)(_navigation|_physics|_render)|_avoidance)?|_global(_file|_dir)|_group|_multiline|_node_path|_placeholder|_range|_subgroup)?").token(T::NameDecorator),
            rule(r"^\s*@(icon|onready|rpc|tool|warning_ignore)").token(T::NameDecorator),
        ]),
        ("types", &[
            rule(r"(?<!\w)(null|void|bool|int|float)\b").token(T::KeywordType),
            rule(r"(?<!\w)(String(Name)?|NodePath|Vector[234]i?|Rect2|Transform[23]D|Plane|Quaternion|AABB|Basis|Color8?|RID|Object|(Packed(Byte|Int(32|64)|Float(32|64)|String|Vector(2|3)|Color))?Array|Dictionary|Signal|Callable)\b").token(T::NameClass),
            rule(r"(?<!\w)(Node|AcceptDialog|AnimatableBody2D|AnimatableBody3D|AnimatedSprite2D|AnimatedSprite3D|AnimationPlayer|AnimationTree|Area2D|Area3D|AspectRatioContainer|AudioListener2D|AudioListener3D|AudioStreamPlayer|AudioStreamPlayer2D|AudioStreamPlayer3D|BackBufferCopy|BaseButton|Bone2D|BoneAttachment3D|BoxContainer|Button|Camera2D|Camera3D|CanvasGroup|CanvasItem|CanvasLayer|CanvasModulate|CenterContainer|CharacterBody2D|CharacterBody3D|CheckBox|CheckButton|CodeEdit|CollisionObject2D|CollisionObject3D|CollisionPolygon2D|CollisionPolygon3D|CollisionShape2D|CollisionShape3D|ColorPicker|ColorPickerButton|ColorRect|ConeTwistJoint3D|ConfirmationDialog|Container|Control|CPUParticles2D|CPUParticles3D|CSGBox3D|CSGCombiner3D|CSGCylinder3D|CSGMesh3D|CSGPolygon3D|CSGPrimitive3D|CSGShape3D|CSGSphere3D|CSGTorus3D|DampedSpringJoint2D|Decal|DirectionalLight2D|DirectionalLight3D|EditorCommandPalette|EditorFileDialog|EditorFileSystem|EditorInspector|EditorPlugin|EditorProperty|EditorResourcePicker|EditorResourcePreview|EditorScriptPicker|EditorSpinSlider|FileDialog|FileSystemDock|FlowContainer|FogVolume|Generic6DOFJoint3D|GeometryInstance3D|GPUParticles2D|GPUParticles3D|GPUParticlesAttractor3D|GPUParticlesAttractorBox3D|GPUParticlesAttractorSphere3D|GPUParticlesAttractorVectorField3D|GPUParticlesCollision3D|GPUParticlesCollisionBox3D|GPUParticlesCollisionHeightField3D|GPUParticlesCollisionSDF3D|GPUParticlesCollisionSphere3D|GraphEdit|GraphNode|GridContainer|GridMap|GrooveJoint2D|HBoxContainer|HFlowContainer|HingeJoint3D|HScrollBar|HSeparator|HSlider|HSplitContainer|HTTPRequest|ImporterMeshInstance3D|InstancePlaceholder|ItemList|Joint2D|Joint3D|Label|Label3D|Light2D|Light3D|LightmapGI|LightmapProbe|LightOccluder2D|Line2D|LineEdit|LinkButton|MarginContainer|Marker2D|Marker3D|MenuBar|MenuButton|MeshInstance2D|MeshInstance3D|MissingNode|MultiMeshInstance2D|MultiMeshInstance3D|MultiplayerSpawner|MultiplayerSynchronizer|NavigationAgent2D|NavigationAgent3D|NavigationLink2D|NavigationLink3D|NavigationObstacle2D|NavigationObstacle3D|NavigationRegion2D|NavigationRegion3D|NinePatchRect|Node2D|Node3D|OccluderInstance3D|OmniLight3D|OpenXRHand|OptionButton|Panel|PanelContainer|ParallaxBackground|ParallaxLayer|Path2D|Path3D|PathFollow2D|PathFollow3D|PhysicalBone2D|PhysicalBone3D|PhysicsBody2D|PhysicsBody3D|PinJoint2D|PinJoint3D|PointLight2D|Polygon2D|Popup|PopupMenu|PopupPanel|ProgressBar|Range|RayCast2D|RayCast3D|ReferenceRect|ReflectionProbe|RemoteTransform2D|RemoteTransform3D|ResourcePreloader|RichTextLabel|RigidBody2D|RigidBody3D|RootMotionView|ScriptCreateDialog|ScriptEditor|ScriptEditorBase|ScrollBar|ScrollContainer|Separator|ShaderGlobalsOverride|ShapeCast2D|ShapeCast3D|Skeleton2D|Skeleton3D|SkeletonIK3D|Slider|SliderJoint3D|SoftBody3D|SpinBox|SplitContainer|SpotLight3D|SpringArm3D|Sprite2D|Sprite3D|SpriteBase3D|StaticBody2D|StaticBody3D|SubViewport|SubViewportContainer|TabBar|TabContainer|TextEdit|TextureButton|TextureProgressBar|TextureRect|TileMap|Timer|TouchScreenButton|Tree|VBoxContainer|VehicleBody3D|VehicleWheel3D|VFlowContainer|VideoStreamPlayer|Viewport|VisibleOnScreenEnabler2D|VisibleOnScreenEnabler3D|VisibleOnScreenNotifier2D|VisibleOnScreenNotifier3D|VisualInstance3D|VoxelGI|VScrollBar|VSeparator|VSlider|VSplitContainer|Window|WorldEnvironment|XRAnchor3D|XRCamera3D|XRController3D|XRNode3D|XROrigin3D)\b").token(T::NameClass),
            rule(r"(?<!\w)(Resource|AnimatedTexture|Animation|AnimationLibrary|AnimationNode|AnimationNodeAdd2|AnimationNodeAdd3|AnimationNodeAnimation|AnimationNodeBlend2|AnimationNodeBlend3|AnimationNodeBlendSpace1D|AnimationNodeBlendSpace2D|AnimationNodeBlendTree|AnimationNodeOneShot|AnimationNodeOutput|AnimationNodeStateMachine|AnimationNodeStateMachinePlayback|AnimationNodeStateMachineTransition|AnimationNodeSub2|AnimationNodeSync|AnimationNodeTimeScale|AnimationNodeTimeSeek|AnimationNodeTransition|AnimationRootNode|ArrayMesh|ArrayOccluder3D|AtlasTexture|AudioBusLayout|AudioEffect|AudioEffectAmplify|AudioEffectBandLimitFilter|AudioEffectBandPassFilter|AudioEffectCapture|AudioEffectChorus|AudioEffectCompressor|AudioEffectDelay|AudioEffectDistortion|AudioEffectEQ|AudioEffectEQ10|AudioEffectEQ21|AudioEffectEQ6|AudioEffectFilter|AudioEffectHighPassFilter|AudioEffectHighShelfFilter|AudioEffectLimiter|AudioEffectLowPassFilter|AudioEffectLowShelfFilter|AudioEffectNotchFilter|AudioEffectPanner|AudioEffectPhaser|AudioEffectPitchShift|AudioEffectRecord|AudioEffectReverb|AudioEffectSpectrumAnalyzer|AudioEffectStereoEnhance|AudioStream|AudioStreamGenerator|AudioStreamMicrophone|AudioStreamMP3|AudioStreamOggVorbis|AudioStreamPolyphonic|AudioStreamRandomizer|AudioStreamWAV|BaseMaterial3D|BitMap|BoneMap|BoxMesh|BoxOccluder3D|BoxShape3D|ButtonGroup|CameraAttributes|CameraAttributesPhysical|CameraAttributesPractical|CameraTexture|CanvasItemMaterial|CanvasTexture|CapsuleMesh|CapsuleShape2D|CapsuleShape3D|CircleShape2D|CodeHighlighter|CompressedCubemap|CompressedCubemapArray|CompressedTexture2D|CompressedTexture2DArray|CompressedTexture3D|CompressedTextureLayered|ConcavePolygonShape2D|ConcavePolygonShape3D|ConvexPolygonShape2D|ConvexPolygonShape3D|CryptoKey|CSharpScript|Cubemap|CubemapArray|Curve|Curve2D|Curve3D|CurveTexture|CurveXYZTexture|CylinderMesh|CylinderShape3D|EditorNode3DGizmoPlugin|EditorSettings|EditorSyntaxHighlighter|Environment|FastNoiseLite|FogMaterial|Font|FontFile|FontVariation|GDExtension|GDScript|GLTFAccessor|GLTFAnimation|GLTFBufferView|GLTFCamera|GLTFDocument|GLTFDocumentExtension|GLTFDocumentExtensionConvertImporterMesh|GLTFLight|GLTFMesh|GLTFNode|GLTFPhysicsBody|GLTFPhysicsShape|GLTFSkeleton|GLTFSkin|GLTFSpecGloss|GLTFState|GLTFTexture|GLTFTextureSampler|Gradient|GradientTexture1D|GradientTexture2D|HeightMapShape3D|Image|ImageTexture|ImageTexture3D|ImageTextureLayered|ImmediateMesh|ImporterMesh|InputEvent|InputEventAction|InputEventFromWindow|InputEventGesture|InputEventJoypadButton|InputEventJoypadMotion|InputEventKey|InputEventMagnifyGesture|InputEventMIDI|InputEventMouse|InputEventMouseButton|InputEventMouseMotion|InputEventPanGesture|InputEventScreenDrag|InputEventScreenTouch|InputEventShortcut|InputEventWithModifiers|JSON|LabelSettings|LightmapGIData|Material|Mesh|MeshLibrary|MeshTexture|MissingResource|MultiMesh|NavigationMesh|NavigationMeshSourceGeometryData3D|NavigationPolygon|Noise|NoiseTexture2D|NoiseTexture3D|Occluder3D|OccluderPolygon2D|OggPacketSequence|OpenXRAction|OpenXRActionMap|OpenXRActionSet|OpenXRInteractionProfile|OpenXRIPBinding|OptimizedTranslation|ORMMaterial3D|PackedDataContainer|PackedScene|PanoramaSkyMaterial|ParticleProcessMaterial|PhysicalSkyMaterial|PhysicsMaterial|PlaceholderCubemap|PlaceholderCubemapArray|PlaceholderMaterial|PlaceholderMesh|PlaceholderTexture2D|PlaceholderTexture2DArray|PlaceholderTexture3D|PlaceholderTextureLayered|PlaneMesh|PointMesh|PolygonOccluder3D|PolygonPathFinder|PortableCompressedTexture2D|PrimitiveMesh|PrismMesh|ProceduralSkyMaterial|QuadMesh|QuadOccluder3D|RDShaderFile|RDShaderSPIRV|RectangleShape2D|RibbonTrailMesh|RichTextEffect|SceneReplicationConfig|Script|ScriptExtension|SegmentShape2D|SeparationRayShape2D|SeparationRayShape3D|Shader|ShaderInclude|ShaderMaterial|Shape2D|Shape3D|Shortcut|SkeletonModification2D|SkeletonModification2DCCDIK|SkeletonModification2DFABRIK|SkeletonModification2DJiggle|SkeletonModification2DLookAt|SkeletonModification2DPhysicalBones|SkeletonModification2DStackHolder|SkeletonModification2DTwoBoneIK|SkeletonModificationStack2D|SkeletonProfile|SkeletonProfileHumanoid|Skin|Sky|SphereMesh|SphereOccluder3D|SphereShape3D|SpriteFrames|StandardMaterial3D|StyleBox|StyleBoxEmpty|StyleBoxFlat|StyleBoxLine|StyleBoxTexture|SyntaxHighlighter|SystemFont|TextMesh|Texture|Texture2D|Texture2DArray|Texture3D|TextureLayered|Theme|TileMapPattern|TileSet|TileSetAtlasSource|TileSetScenesCollectionSource|TileSetSource|TorusMesh|Translation|TubeTrailMesh|VideoStream|VideoStreamPlayback|VideoStreamTheora|ViewportTexture|VisualShader|VisualShaderNode|VisualShaderNodeBillboard|VisualShaderNodeBooleanConstant|VisualShaderNodeBooleanParameter|VisualShaderNodeClamp|VisualShaderNodeColorConstant|VisualShaderNodeColorFunc|VisualShaderNodeColorOp|VisualShaderNodeColorParameter|VisualShaderNodeComment|VisualShaderNodeCompare|VisualShaderNodeConstant|VisualShaderNodeCubemap|VisualShaderNodeCubemapParameter|VisualShaderNodeCurveTexture|VisualShaderNodeCurveXYZTexture|VisualShaderNodeCustom|VisualShaderNodeDerivativeFunc|VisualShaderNodeDeterminant|VisualShaderNodeDistanceFade|VisualShaderNodeDotProduct|VisualShaderNodeExpression|VisualShaderNodeFaceForward|VisualShaderNodeFloatConstant|VisualShaderNodeFloatFunc|VisualShaderNodeFloatOp|VisualShaderNodeFloatParameter|VisualShaderNodeFresnel|VisualShaderNodeGlobalExpression|VisualShaderNodeGroupBase|VisualShaderNodeIf|VisualShaderNodeInput|VisualShaderNodeIntConstant|VisualShaderNodeIntFunc|VisualShaderNodeIntOp|VisualShaderNodeIntParameter|VisualShaderNodeIs|VisualShaderNodeLinearSceneDepth|VisualShaderNodeMix|VisualShaderNodeMultiplyAdd|VisualShaderNodeOuterProduct|VisualShaderNodeOutput|VisualShaderNodeParameter|VisualShaderNodeParameterRef|VisualShaderNodeParticleAccelerator|VisualShaderNodeParticleBoxEmitter|VisualShaderNodeParticleConeVelocity|VisualShaderNodeParticleEmit|VisualShaderNodeParticleEmitter|VisualShaderNodeParticleMeshEmitter|VisualShaderNodeParticleMultiplyByAxisAngle|VisualShaderNodeParticleOutput|VisualShaderNodeParticleRandomness|VisualShaderNodeParticleRingEmitter|VisualShaderNodeParticleSphereEmitter|VisualShaderNodeProximityFade|VisualShaderNodeRandomRange|VisualShaderNodeRemap|VisualShaderNodeResizableBase|VisualShaderNodeSample3D|VisualShaderNodeScreenUVToSDF|VisualShaderNodeSDFRaymarch|VisualShaderNodeSDFToScreenUV|VisualShaderNodeSmoothStep|VisualShaderNodeStep|VisualShaderNodeSwitch|VisualShaderNodeTexture|VisualShaderNodeTexture2DArray|VisualShaderNodeTexture2DArrayParameter|VisualShaderNodeTexture2DParameter|VisualShaderNodeTexture3D|VisualShaderNodeTexture3DParameter|VisualShaderNodeTextureParameter|VisualShaderNodeTextureParameterTriplanar|VisualShaderNodeTextureSDF|VisualShaderNodeTextureSDFNormal|VisualShaderNodeTransformCompose|VisualShaderNodeTransformConstant|VisualShaderNodeTransformDecompose|VisualShaderNodeTransformFunc|VisualShaderNodeTransformOp|VisualShaderNodeTransformParameter|VisualShaderNodeTransformVecMult|VisualShaderNodeUIntConstant|VisualShaderNodeUIntFunc|VisualShaderNodeUIntOp|VisualShaderNodeUIntParameter|VisualShaderNodeUVFunc|VisualShaderNodeUVPolarCoord|VisualShaderNodeVarying|VisualShaderNodeVaryingGetter|VisualShaderNodeVaryingSetter|VisualShaderNodeVec2Constant|VisualShaderNodeVec2Parameter|VisualShaderNodeVec3Constant|VisualShaderNodeVec3Parameter|VisualShaderNodeVec4Constant|VisualShaderNodeVec4Parameter|VisualShaderNodeVectorBase|VisualShaderNodeVectorCompose|VisualShaderNodeVectorDecompose|VisualShaderNodeVectorDistance|VisualShaderNodeVectorFunc|VisualShaderNodeVectorLen|VisualShaderNodeVectorOp|VisualShaderNodeVectorRefract|VoxelGIData|World2D|World3D|WorldBoundaryShape2D|WorldBoundaryShape3D|X509Certificate)\b").token(T::NameClass),
            rule(r"(?<!\w)(Object|AESContext|AStar2D|AStar3D|AStarGrid2D|AudioEffectInstance|AudioEffectSpectrumAnalyzerInstance|AudioServer|AudioStreamGeneratorPlayback|AudioStreamPlayback|AudioStreamPlaybackOggVorbis|AudioStreamPlaybackPolyphonic|AudioStreamPlaybackResampled|CallbackTweener|CameraFeed|CameraServer|CharFXTransform|ClassDB|ConfigFile|Crypto|DirAccess|DisplayServer|DTLSServer|EditorDebuggerPlugin|EditorDebuggerSession|EditorExportPlatform|EditorExportPlatformAndroid|EditorExportPlatformIOS|EditorExportPlatformLinuxBSD|EditorExportPlatformMacOS|EditorExportPlatformPC|EditorExportPlatformWeb|EditorExportPlatformWindows|EditorExportPlugin|EditorFeatureProfile|EditorFileSystemDirectory|EditorFileSystemImportFormatSupportQuery|EditorImportPlugin|EditorInspectorPlugin|EditorInterface|EditorNode3DGizmo|EditorPaths|EditorResourceConversionPlugin|EditorResourcePreviewGenerator|EditorResourceTooltipPlugin|EditorSceneFormatImporter|EditorSceneFormatImporterBlend|EditorSceneFormatImporterFBX|EditorSceneFormatImporterGLTF|EditorScenePostImport|EditorScenePostImportPlugin|EditorScript|EditorSelection|EditorTranslationParserPlugin|EditorUndoRedoManager|EditorVCSInterface|EncodedObjectAsID|ENetConnection|ENetMultiplayerPeer|ENetPacketPeer|Engine|EngineDebugger|EngineProfiler|Expression|FileAccess|GDExtensionManager|Geometry2D|Geometry3D|GodotSharp|HashingContext|HMACContext|HTTPClient|ImageFormatLoader|ImageFormatLoaderExtension|Input|InputMap|IntervalTweener|IP|JavaClass|JavaClassWrapper|JavaScriptBridge|JavaScriptObject|JNISingleton|JSONRPC|KinematicCollision2D|KinematicCollision3D|Lightmapper|LightmapperRD|MainLoop|Marshalls|MeshConvexDecompositionSettings|MeshDataTool|MethodTweener|MobileVRInterface|MovieWriter|MultiplayerAPI|MultiplayerAPIExtension|MultiplayerPeer|MultiplayerPeerExtension|Mutex|NavigationMeshGenerator|NavigationPathQueryParameters2D|NavigationPathQueryParameters3D|NavigationPathQueryResult2D|NavigationPathQueryResult3D|NavigationServer2D|NavigationServer3D|Node|Node3DGizmo|OfflineMultiplayerPeer|OggPacketSequencePlayback|OpenXRInterface|OS|PackedDataContainerRef|PacketPeer|PacketPeerDTLS|PacketPeerExtension|PacketPeerStream|PacketPeerUDP|PCKPacker|Performance|PhysicsDirectBodyState2D|PhysicsDirectBodyState2DExtension|PhysicsDirectBodyState3D|PhysicsDirectBodyState3DExtension|PhysicsDirectSpaceState2D|PhysicsDirectSpaceState2DExtension|PhysicsDirectSpaceState3D|PhysicsDirectSpaceState3DExtension|PhysicsPointQueryParameters2D|PhysicsPointQueryParameters3D|PhysicsRayQueryParameters2D|PhysicsRayQueryParameters3D|PhysicsServer2D|PhysicsServer2DExtension|PhysicsServer2DManager|PhysicsServer3D|PhysicsServer3DExtension|PhysicsServer3DManager|PhysicsServer3DRenderingServerHandler|PhysicsShapeQueryParameters2D|PhysicsShapeQueryParameters3D|PhysicsTestMotionParameters2D|PhysicsTestMotionParameters3D|PhysicsTestMotionResult2D|PhysicsTestMotionResult3D|ProjectSettings|PropertyTweener|RandomNumberGenerator|RDAttachmentFormat|RDFramebufferPass|RDPipelineColorBlendState|RDPipelineColorBlendStateAttachment|RDPipelineDepthStencilState|RDPipelineMultisampleState|RDPipelineRasterizationState|RDPipelineSpecializationConstant|RDSamplerState|RDShaderSource|RDTextureFormat|RDTextureView|RDUniform|RDVertexAttribute|RefCounted|RegEx|RegExMatch|RenderingDevice|RenderingServer|Resource|ResourceFormatLoader|ResourceFormatSaver|ResourceImporter|ResourceLoader|ResourceSaver|ResourceUID|SceneMultiplayer|SceneState|SceneTree|SceneTreeTimer|ScriptLanguage|ScriptLanguageExtension|Semaphore|SkinReference|StreamPeer|StreamPeerBuffer|StreamPeerExtension|StreamPeerGZIP|StreamPeerTCP|StreamPeerTLS|SurfaceTool|TCPServer|TextLine|TextParagraph|TextServer|TextServerAdvanced|TextServerDummy|TextServerExtension|TextServerFallback|TextServerManager|ThemeDB|Thread|TileData|Time|TLSOptions|TranslationServer|TreeItem|TriangleMesh|Tween|Tweener|UDPServer|UndoRedo|UPNP|UPNPDevice|WeakRef|WebRTCDataChannel|WebRTCDataChannelExtension|WebRTCMultiplayerPeer|WebRTCPeerConnection|WebRTCPeerConnectionExtension|WebSocketMultiplayerPeer|WebSocketPeer|WebXRInterface|WorkerThreadPool|XMLParser|XRInterface|XRInterfaceExtension|XRPose|XRPositionalTracker|XRServer|ZIPPacker|ZIPReader)\b").token(T::NameClass),
            rule(r"(?<!\w)(EditorCommandPalette|EditorDebuggerPlugin|EditorDebuggerSession|EditorExportPlatform|EditorExportPlatformAndroid|EditorExportPlatformIOS|EditorExportPlatformLinuxBSD|EditorExportPlatformMacOS|EditorExportPlatformPC|EditorExportPlatformWeb|EditorExportPlatformWindows|EditorExportPlugin|EditorFeatureProfile|EditorFileDialog|EditorFileSystem|EditorFileSystemDirectory|EditorFileSystemImportFormatSupportQuery|EditorImportPlugin|EditorInspector|EditorInspectorPlugin|EditorInterface|EditorNode3DGizmo|EditorNode3DGizmoPlugin|EditorPaths|EditorPlugin|EditorProperty|EditorResourceConversionPlugin|EditorResourcePicker|EditorResourcePreview|EditorResourcePreviewGenerator|EditorResourceTooltipPlugin|EditorSceneFormatImporter|EditorSceneFormatImporterBlend|EditorSceneFormatImporterFBX|EditorSceneFormatImporterGLTF|EditorScenePostImport|EditorScenePostImportPlugin|EditorScript|EditorScriptPicker|EditorSelection|EditorSettings|EditorSpinSlider|EditorSyntaxHighlighter|EditorTranslationParserPlugin|EditorUndoRedoManager|EditorVCSInterface|FileSystemDock|ScriptCreateDialog|ScriptEditor|ScriptEditorBase)\b").token(T::NameClass),
        ]),
        ("strings_double", &[
            include("strings"),
            rule(r#"\{[^\\\"\n]*\}"#).token(T::LiteralStringInterpol),
            rule(r#"[^\\\"\{%]+"#).token(T::LiteralStringDouble),
            rule(r"%").token(T::LiteralStringDouble),
            rule(r"{").token(T::LiteralStringDouble),
        ]),
        ("strings", &[
            rule(r"%(?:[+-]?[0-9*]*\.?[0-9*]*)?[scdoxXf]").token(T::LiteralStringInterpol),
            rule(r#"\\(U[0-9a-fA-F]{6}|u[0-9a-fA-F]{4}|[\n\\\'\"ntrabfv])"#).token(T::LiteralStringEscape),
        ]),
        ("root", &[
            rule(r"\n").token(T::Text),
            rule(r"[^\S\n]+").token(T::Text),
            rule(r"#.*$").token(T::CommentSingle),
            rule(r"\\\n").token(T::Text),
            include("keywords"),
            include("annotations"),
            rule(r"\*\*|[*\/~+-]|<<|>>|[&^|]|==|!=|[<>]|<=|>=|!|&&|\|\||=|:=|\+=|-=|\*=|\/=|\*\*=|%=|&=|\|=|\^=|<<=|>>=|->|\.").token(T::Operator),
            rule(r"[]{}:(),;[]").token(T::Punctuation),
            rule(r"(func)((?:\s|\\\s)+)").groups(&[T::KeywordDeclaration, T::Text]).push(&["funcname"]),
            rule(r"(class|class_name|extends)((?:\s|\\\s)+)").groups(&[T::KeywordDeclaration, T::Text]).push(&["classname"]),
            rule(r"\$[a-zA-Z_][\w\/]*").token(T::NameOther),
            rule(r"%[a-zA-Z_]\w*").token(T::NameOther),
            rule(r"%").token(T::Operator),
            include("types"),
            include("builtin_funcs"),
            include("numbers"),
            rule(r"(\b[a-zA-Z_]\w*)([(])").groups(&[T::NameFunction, T::Punctuation]),
            rule(r"'").token(T::LiteralStringSingle).push(&["sqs"]),
            rule(r#"""#).token(T::LiteralStringDouble).push(&["dqs"]),
            rule(r"'''").token(T::LiteralStringSingle).push(&["tsqs"]),
            rule(r#"""""#).token(T::LiteralStringDouble).push(&["tdqs"]),
            rule(r"[a-zA-Z_]\w*").token(T::Name),
        ]),
    ],
};
